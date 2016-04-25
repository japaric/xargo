use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use cargo::util::{self, CargoResult, ChainError, Config, Filesystem};
use chrono::NaiveDate;
use curl::http;
use flate2::read::GzDecoder;
use rustc_version::{self, Channel};
use tar::Archive;
use tempdir::TempDir;
use term::color::GREEN;

use Target;

/// Create a sysroot that looks like this:
///
/// ``` text
/// ~/.xargo
/// ├── date
/// ├── lib
/// │   └── rustlib
/// │       ├── $HOST
/// │       │   └── lib
/// │       │       ├── libcore-$hash.rlib -> $SYSROOT/lib/rustlib/$HOST/lib/libcore-$hash.rlib
/// │       │       └── (..)
/// │       ├── $TARGET1
/// │       │   ├── hash
/// │       │   └── lib
/// │       │       ├── libcore-$hash.rlib
/// │       │       └── (..)
/// │       ├── $TARGET2
/// │       │   └── (..)
/// │       ├── (..)
/// │       └── $TARGETN
/// │           └── (..)
/// └── src
///     │── libcore
///     │── libstd
///     └── (..)
/// ```
///
/// Where:
///
/// - `$SYSROOT` is the current `rustc` sysroot i.e. `$(rustc --print sysroot)`
/// - `$HOST` is the current `rustc`'s host i.e. the host field in `$(rustc -Vv)`
/// - `$TARGET*` are the custom targets `xargo` is managing
///
/// The `~/.xargo` is mostly a "standard" sysroot but with extra information:
///
/// - the `hash` files which track changes in the `$TARGET`s' specification files (i.e.
/// `$TARGET.json`).
/// - the `src` directory which holds the source code of the current `rustc` (and standard crates).
/// - the `date` file which holds the build date of the current `rustc`.
pub fn create(config: &Config,
              target: &Target,
              root: &Filesystem,
              verbose: bool)
              -> CargoResult<()> {
    let meta = rustc_version::version_meta_for(&config.rustc_info().verbose_version);

    if meta.channel != Channel::Nightly {
        return Err(util::human("Only the nightly channel is currently supported"));
    }

    let commit_date = try!(meta.commit_date
                               .as_ref()
                               .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                               .ok_or(util::human("couldn't find/parse the commit date from \
                                                   `rustc -Vv`")));
    // XXX AFAIK this is not guaranteed to be correct, but it appears to be a good approximation.
    let build_date = commit_date.succ();

    try!(update_source(config, &build_date, root));
    try!(rebuild_sysroot(config, root, target, verbose));
    try!(symlink_host_crates(config, root));

    Ok(())
}

fn update_source(config: &Config, date: &NaiveDate, root: &Filesystem) -> CargoResult<()> {
    const TARBALL: &'static str = "rustc-nightly-src.tar.gz";

    /// Reads the `NaiveDate` stored in `~/.xargo/date`
    fn read_date(mut file: &File) -> CargoResult<Option<NaiveDate>> {
        let date = &mut String::new();
        try!(file.read_to_string(date));

        Ok(NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
    }

    fn download(config: &Config, date: &NaiveDate) -> CargoResult<http::Response> {
        const B_PER_S: usize = 1;
        const MS: usize = 1_000;
        const S: usize = 1;

        // NOTE Got these settings from cargo (src/cargo/ops/registry.rs)
        let mut handle = http::handle()
                             .timeout(0)
                             .connect_timeout(30 * MS)
                             .low_speed_limit(10 * B_PER_S)
                             .low_speed_timeout(30 * S);;

        let url = format!("https://static.rust-lang.org/dist/{}/{}",
                          date.format("%Y-%m-%d"),
                          TARBALL);

        try!(config.shell().out().say_status("Downloading", &url, GREEN, true));
        let resp = try!(handle.get(url).follow_redirects(true).exec());

        let code = resp.get_code();
        if code != 200 {
            return Err(util::human(format!("HTTP error got {}, expected 200", code)));
        }

        Ok(resp)
    }

    fn unpack(config: &Config, tarball: http::Response, root: &Path) -> CargoResult<()> {
        try!(config.shell().out().say_status("Unpacking", TARBALL, GREEN, true));

        let src_dir = &root.join("src");
        try!(fs::create_dir(src_dir));

        let decoder = try!(GzDecoder::new(tarball.get_body()));
        let mut archive = Archive::new(decoder);
        for entry in try!(archive.entries()) {
            let mut entry = try!(entry);
            let path = {
                let path = try!(entry.path());
                let mut components = path.components();
                components.next();
                let next = components.next().and_then(|s| s.as_os_str().to_str());
                if next != Some("src") {
                    continue;
                }
                components.as_path().to_path_buf()
            };
            try!(entry.unpack(src_dir.join(path)));
        }

        Ok(())
    }

    let lock = try!(root.open_rw("date", config, "xargo"));

    if try!(read_date(lock.file())).as_ref() == Some(date) {
        // Source is up to date
        return Ok(());
    }

    try!(lock.remove_siblings());
    let tarball = try!(download(config, date)
                           .chain_error(|| util::human("Couldn't fetch Rust source tarball")));
    try!(unpack(config, tarball, lock.parent())
             .chain_error(|| util::human("Couldn't unpack Rust source tarball")));

    let mut file = lock.file();
    try!(file.set_len(0));
    try!(file.write_all(date.format("%Y-%m-%d").to_string().as_bytes()));

    Ok(())
}

fn rebuild_sysroot(config: &Config,
                   root: &Filesystem,
                   target: &Target,
                   verbose: bool)
                   -> CargoResult<()> {
    /// Reads the hash stored in `~/.xargo/lib/rustlib/$TARGET/hash`
    fn read_hash(mut file: &File) -> CargoResult<Option<u64>> {
        let hash = &mut String::new();
        try!(file.read_to_string(hash));
        Ok(hash.parse().ok())
    }

    const CRATES: &'static [&'static str] = &["collections", "rand"];
    const TOML: &'static str = "[package]
name = 'sysroot'
version = '0.0.0'

[dependencies]
";

    let outer_lock = try!(root.open_ro("date", config, "xargo"));
    let lock = try!(root.open_rw(format!("lib/rustlib/{}/hash", target.triple),
                                 config,
                                 &format!("xargo/{}", target.triple)));
    let root = outer_lock.parent();

    if try!(read_hash(lock.file())) == Some(target.hash) {
        // Target specification file unchanged
        return Ok(());
    }

    let lib_dir = &lock.parent().join("lib");
    try!(config.shell().out().say_status("Compiling",
                                         format!("sysroot for {}", target.triple),
                                         GREEN,
                                         true));

    let td = try!(TempDir::new("xargo"));
    let td = td.path();

    // Create Cargo project
    try!(fs::create_dir(td.join("src")));
    try!(fs::copy(&target.path, td.join(target.path.file_name().unwrap())));
    try!(File::create(td.join("src/lib.rs")));
    let toml = &mut String::from(TOML);
    for krate in CRATES {
        toml.push_str(&format!("{} = {{ path = '{}' }}\n",
                               krate,
                               root.join(format!("src/lib{}", krate)).display()))
    }
    try!(try!(File::create(td.join("Cargo.toml"))).write_all(toml.as_bytes()));
    if let Some(rustflags) = try!(config.get_list("build.rustflags")) {
        try!(fs::create_dir(td.join(".cargo")));
        try!(try!(File::create(td.join(".cargo/config")))
                 .write_all(format!("[build]\nrustflags = {:?}",
                                    rustflags.val.into_iter().map(|t| t.0).collect::<Vec<_>>())
                                .as_bytes()));
    }

    // Build Cargo project
    let cargo = &mut Command::new("cargo");
    cargo.args(&["build", "--release", "--target"]);
    cargo.arg(&target.triple);
    if verbose {
        cargo.arg("--verbose");
    }
    for krate in CRATES {
        cargo.args(&["-p", krate]);
    }
    cargo.current_dir(td);
    let status = try!(cargo.status());

    if !status.success() {
        return Err(util::human("`cargo` process didn't exit successfully"));
    }

    // Copy build artifacts
    if lib_dir.exists() {
        try!(fs::remove_dir_all(lib_dir));
    }
    let dst = lib_dir;
    try!(fs::create_dir_all(dst));
    for entry in try!(fs::read_dir(td.join(format!("target/{}/release/deps", target.triple)))) {
        let src = &try!(entry).path();
        try!(fs::copy(src, dst.join(src.file_name().unwrap())));
    }

    let mut file = lock.file();
    try!(file.set_len(0));
    try!(file.write_all(target.hash.to_string().as_bytes()));

    Ok(())
}

fn symlink_host_crates(config: &Config, root: &Filesystem) -> CargoResult<()> {
    let _outer_lock = try!(root.open_ro("date", config, "xargo"));
    let host = &config.rustc_info().host;
    let lock = try!(root.open_rw(format!("lib/rustlib/{}/sentinel", host),
                                 config,
                                 &format!("xargo/{}", host)));
    let dst = &lock.parent().join("lib");

    if dst.exists() {
        return Ok(());
    }

    try!(fs::create_dir_all(dst));
    let src = try!(sysroot()).join(format!("lib/rustlib/{}/lib", host));

    for entry in try!(fs::read_dir(src)) {
        let src = &try!(entry).path();

        try!(fs::hard_link(src, dst.join(src.file_name().unwrap())));
    }

    Ok(())
}

fn sysroot() -> CargoResult<PathBuf> {
    let mut sysroot = try!(String::from_utf8(try!(Command::new("rustc")
                                                      .args(&["--print", "sysroot"])
                                                      .output())
                                                 .stdout)
                               .map_err(|_| {
                                   util::human("output of `rustc --print sysroot` is not UTF-8")
                               }));

    while sysroot.ends_with('\n') {
        sysroot.pop();
    }

    Ok(PathBuf::from(sysroot))
}
