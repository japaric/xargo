use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use cargo::util::{self, CargoResult, ChainError, Config, FileLock, Filesystem};
use chrono::NaiveDate;
use curl::http;
use flate2::read::GzDecoder;
use tar::Archive;
use rustc_version::VersionMeta;
use tempdir::TempDir;
use term::color::GREEN;
use toml::Value;

use Target;

enum Source {
    Rustup(PathBuf),
    Xargo,
}

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
              verbose: bool,
              rustflags: &[String],
              profiles: &Option<Value>,
              meta: VersionMeta)
              -> CargoResult<()> {
    let commit_date = try!(meta.commit_date
        .as_ref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .ok_or(util::human("couldn't find/parse the commit date in/from `rustc -Vv`")));
    // XXX AFAIK this is not guaranteed to be correct, but it appears to be a good approximation.
    let build_date = commit_date.succ();
    let commit_hash = try!(meta.commit_hash
        .ok_or(util::human("couldn't find the commit hash in `rustc -Vv`")));

    let src = try!(update_source(config, &commit_hash, &build_date, root));
    try!(rebuild_sysroot(config, root, target, verbose, rustflags, profiles, src));
    try!(copy_host_crates(config, root));

    Ok(())
}

fn update_source(config: &Config,
                 commit_hash: &str,
                 date: &NaiveDate,
                 root: &Filesystem)
                 -> CargoResult<Source> {
    const TARBALL: &'static str = "rustc-nightly-src.tar.gz";

    /// Reads the `NaiveDate` stored in `~/.xargo/date`
    fn read_date(file: &File) -> CargoResult<Option<NaiveDate>> {
        read_string(file).map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
    }

    fn read_string(mut file: &File) -> CargoResult<String> {
        let mut s = String::new();
        try!(file.read_to_string(&mut s));
        Ok(s)
    }

    fn write_to_lockfile(lock: &FileLock, contents: &str) -> CargoResult<()> {
        // NOTE locked files are open in append mode, we need to "rewind" to the start before
        // writing
        let mut file = lock.file();
        try!(file.seek(SeekFrom::Start(0)));
        try!(file.set_len(0));
        try!(file.write_all(contents.as_bytes()));
        Ok(())
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
            .low_speed_timeout(30 * S);

        let url = format!("https://static.rust-lang.org/dist/{}/{}",
                          date.format("%Y-%m-%d"),
                          TARBALL);

        try!(config.shell().err().say_status("Downloading", &url, GREEN, true));
        let resp = try!(handle.get(url).follow_redirects(true).exec());

        let code = resp.get_code();
        if code != 200 {
            return Err(util::human(format!("HTTP error got {}, expected 200", code)));
        }

        Ok(resp)
    }

    fn unpack(config: &Config, tarball: http::Response, root: &Path) -> CargoResult<()> {
        try!(config.shell().err().say_status("Unpacking", TARBALL, GREEN, true));

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

    let rustup_src_dir = try!(sysroot()).join("lib/rustlib/src/rust");

    if rustup_src_dir.exists() {
        let xargo_src_dir = &lock.path().parent().unwrap().join("src");

        if xargo_src_dir.exists() {
            try!(fs::remove_dir_all(xargo_src_dir));
        }

        if try!(read_string(lock.file())) != commit_hash {
            // Outdated build artifacts, remove them.
            try!(lock.remove_siblings());

            // Use the commit hash as a "timestamp" to detect rustc updates
            try!(write_to_lockfile(&lock, commit_hash));
        }

        return Ok(Source::Rustup(rustup_src_dir));
    }

    if try!(read_date(lock.file())).as_ref() == Some(date) {
        // Source is up to date
        return Ok(Source::Xargo);
    }

    try!(lock.remove_siblings());
    let tarball = try!(download(config, date)
        .chain_error(|| util::human("Couldn't fetch Rust source tarball")));
    try!(unpack(config, tarball, lock.parent())
        .chain_error(|| util::human("Couldn't unpack Rust source tarball")));

    // Leave a timestamp around to indicate how old this source is
    try!(write_to_lockfile(&lock, &date.format("%Y-%m-%d").to_string()));

    Ok(Source::Xargo)
}

fn rebuild_sysroot(config: &Config,
                   root: &Filesystem,
                   target: &Target,
                   verbose: bool,
                   rustflags: &[String],
                   profiles: &Option<Value>,
                   src: Source)
                   -> CargoResult<()> {
    /// Reads the hash stored in `~/.xargo/lib/rustlib/$TARGET/hash`
    fn read_hash(mut file: &File) -> CargoResult<Option<u64>> {
        let hash = &mut String::new();
        try!(file.read_to_string(hash));
        Ok(hash.parse().ok())
    }

    const CRATES: &'static [&'static str] = &["collections", "rand"];
    const NO_ATOMICS_CRATES: &'static [&'static str] = &["rustc_unicode"];

    let outer_lock = try!(root.open_ro("date", config, "xargo"));
    let lock = try!(root.open_rw(format!("lib/rustlib/{}/hash", target.triple),
                                 config,
                                 &format!("xargo/{}", target.triple)));
    let root = outer_lock.parent();

    let mut hasher = target.hasher.clone();
    rustflags.hash(&mut hasher);
    if let Some(profiles) = profiles.as_ref() {
        profiles.to_string().hash(&mut hasher);
    }
    let hash = hasher.finish();
    if try!(read_hash(lock.file())) == Some(hash) {
        // Target specification file unchanged
        return Ok(());
    }

    let lib_dir = &lock.parent().join("lib");
    try!(config.shell().err().say_status("Compiling",
                                         format!("sysroot for {}", target.triple),
                                         GREEN,
                                         true));

    let td = try!(TempDir::new("xargo"));
    let td = td.path();

    // Create Cargo project
    try!(fs::create_dir(td.join("src")));
    try!(fs::copy(&target.path, td.join(target.path.file_name().unwrap())));
    try!(File::create(td.join("src/lib.rs")));
    let toml = &mut format!("[package]
name = 'sysroot'
version = '0.0.0'

{}

[dependencies]
",
                            profiles.as_ref().map(|t| t.to_string()).unwrap_or(String::new()));
    let src_dir = &match src {
        Source::Rustup(dir) => dir.join("src"),
        Source::Xargo => root.join("src"),
    };
    for krate in CRATES {
        toml.push_str(&format!("{} = {{ path = '{}' }}\n",
                               krate,
                               src_dir.join(format!("lib{}", krate)).display()))
    }
    try!(try!(File::create(td.join("Cargo.toml"))).write_all(toml.as_bytes()));
    if !rustflags.is_empty() {
        try!(fs::create_dir(td.join(".cargo")));
        try!(try!(File::create(td.join(".cargo/config")))
            .write_all(format!("[build]\nrustflags = {:?}", rustflags).as_bytes()));
    }

    // Build Cargo project
    let cargo = &mut Command::new("cargo");
    cargo.args(&["build", "--release", "--target"]);
    cargo.arg(&target.triple);
    if verbose {
        cargo.arg("--verbose");
    }
    if target.cfg.target_has_atomic.is_empty() {
        for krate in NO_ATOMICS_CRATES {
            cargo.args(&["-p", krate]);
        }
    } else {
        for krate in CRATES {
            cargo.args(&["-p", krate]);
        }
    }
    cargo.env("CARGO_TARGET_DIR", td.join("target"));
    cargo.arg("--manifest-path").arg(td.join("Cargo.toml"));
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
    try!(file.seek(SeekFrom::Start(0)));
    try!(file.set_len(0));
    try!(file.write_all(hash.to_string().as_bytes()));

    Ok(())
}

fn copy_host_crates(config: &Config, root: &Filesystem) -> CargoResult<()> {
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

        try!(fs::copy(src, dst.join(src.file_name().unwrap())));
    }

    Ok(())
}

fn sysroot() -> CargoResult<PathBuf> {
    let mut sysroot = try!(String::from_utf8(try!(Command::new("rustc")
                .args(&["--print", "sysroot"])
                .output())
            .stdout)
        .map_err(|_| util::human("output of `rustc --print sysroot` is not UTF-8")));

    while sysroot.ends_with('\n') {
        sysroot.pop();
    }

    Ok(PathBuf::from(sysroot))
}
