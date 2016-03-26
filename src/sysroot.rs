use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use chrono::NaiveDate;
use curl::http::Handle;
use flate2::read::GzDecoder;
use rustc_version::{self, Channel};
use tar::Archive;
use tempdir::TempDir;

use Target;

pub fn create(target: &Target) -> PathBuf {
    let meta = rustc_version::version_meta();

    if meta.channel != Channel::Nightly {
        panic!("Only the nightly channel is currently supported");
    }

    let home = &env::home_dir().unwrap();
    let commit_date = NaiveDate::parse_from_str(meta.commit_date.as_ref().unwrap(), "%Y-%m-%d")
                          .unwrap();
    // XXX AFAIK this is not guaranteed to be correct, but it appears to be a good approximation
    let build_date = commit_date.succ();
    let build_date_ = &build_date.format("%Y-%m-%d").to_string();
    info!("rustc is a nightly from: {}", build_date_);

    let root = Path::new(home).join(".xargo");
    let source_date = read_date(&root).ok();

    if let Some(date) = source_date {
        info!("cached nightly source is from: {}", date);
    }

    if source_date != Some(build_date) {
        fetch_source(&root, build_date_);
    }

    let prev_hash = read_hash(&root, target).ok();
    if prev_hash != Some(target.hash) {
        if prev_hash.is_some() {
            info!("target specification file hash changed");
        }

        build_crates(&root, target);
    }

    if !root.join(format!("lib/rustlib/{}", meta.host)).exists() {
        symlink_host_crates(&meta.host, &root);
    }

    root
}

/// Reads `~/.xargo/date`
fn read_date(xargo_dir: &Path) -> io::Result<NaiveDate> {
    let date = &mut String::new();
    File::open(xargo_dir.join("date"))
        .and_then(|mut f| f.read_to_string(date))
        .map(|_| try!(NaiveDate::parse_from_str(date, "%Y-%m-%d")))
}

/// Reads `~/.xargo/lib/rustlib/{target}/hash`
fn read_hash(root: &Path, target: &Target) -> io::Result<u64> {
    let hash = &mut String::new();
    File::open(root.join(format!("lib/rustlib/{}/hash", target.triple)))
        .and_then(|mut f| f.read_to_string(hash))
        .map(|_| try!(hash.parse()))
}

/// Fetches nightly source from `date` into `$root/src`
fn fetch_source(root: &Path, date: &str) {
    if root.exists() {
        info!("purging ~/.xargo");
        try!(fs::remove_dir_all(root));
    }

    info!("fetching source tarball");
    let handle = Handle::new();
    let url = format!("https://static.rust-lang.org/dist/{}/rustc-nightly-src.tar.gz",
                      date);
    let resp = try!(handle.timeout(600_000).get(url).follow_redirects(true).exec());

    assert_eq!(resp.get_code(), 200);

    info!("unpacking source tarball");
    let src_dir = &root.join("src");
    try!(fs::create_dir_all(src_dir));

    let decoder = try!(GzDecoder::new(resp.get_body()));
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
    try!(try!(File::create(root.join("date"))).write_all(date.as_bytes()));
}

fn build_crates(root: &Path, target: &Target) {
    // NOTE These crates pull all the other free-standing crates
    const CRATES: &'static [&'static str] = &["collections", "rand"];
    const CARGO_TOML: &'static str = "\
[package]
name = 'sysroot'
version = '0.0.0'

[dependencies]
";
    info!("cross compiling crates");

    let temp_dir_ = try!(TempDir::new("xargo"));
    let temp_dir = temp_dir_.path();

    // Create Cargo project
    try!(fs::create_dir(temp_dir.join("src")));
    try!(fs::copy(&target.path,
                  temp_dir.join(target.path.file_name().unwrap())));
    try!(File::create(temp_dir.join("src/lib.rs")));
    let toml = &mut String::from(CARGO_TOML);
    for krate in CRATES {
        toml.push_str(&format!("{} = {{ path = '{}' }}\n",
                               krate,
                               root.join(format!("src/lib{}", krate)).display()));
    }
    try!(try!(File::create(temp_dir.join("Cargo.toml"))).write_all(toml.as_bytes()));

    let cargo = &mut Command::new("cargo");
    cargo.args(&["build", "--release", "--target"]);
    cargo.arg(&target.triple);
    for krate in CRATES {
        cargo.args(&["-p", krate]);
    }
    cargo.current_dir(temp_dir);
    info!("calling: {:?}", cargo);
    let output = try!(cargo.output());

    if output.status.success() {
        info!("\n{}", String::from_utf8_lossy(&output.stdout));
    } else {
        info!("\n{}", String::from_utf8_lossy(&output.stdout));
        error!("\n{}", String::from_utf8_lossy(&output.stderr));

        process::exit(output.status.code().unwrap_or(1));
    }

    let dst_dir = &root.join(format!("lib/rustlib/{}/lib", target.triple));
    try!(fs::create_dir_all(dst_dir));
    for entry in try!(fs::read_dir(temp_dir.join(format!("target/{}/release/deps",
                                                         target.triple)))) {
        let src = &try!(entry).path();
        try!(fs::copy(src, dst_dir.join(src.file_name().unwrap())));
    }

    try!(try!(File::create(root.join(format!("lib/rustlib/{}/hash", target.triple))))
             .write_all(target.hash.to_string().as_bytes()));
}

fn symlink_host_crates(host: &str, hash_dir: &Path) {
    info!("symlinking host crates");

    let sysroot = try!(String::from_utf8(try!(Command::new("rustc")
                                                  .args(&["--print", "sysroot"])
                                                  .output())
                                             .stdout));
    let src = &PathBuf::from(sysroot.trim_right()).join(format!("lib/rustlib/{}/lib", host));
    let dst = hash_dir.join(format!("lib/rustlib/{}/lib", host));

    link_dir(src, &dst);
}

fn link_dir(src_dir: &Path, dst_dir: &Path) {
    try!(fs::create_dir_all(dst_dir));

    for entry in try!(fs::read_dir(src_dir)) {
        let path = &try!(entry).path();

        try!(fs::hard_link(path, dst_dir.join(path.file_name().unwrap())));
    }
}
