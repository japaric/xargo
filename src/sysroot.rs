use std::hash::{Hash, Hasher, SipHasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use tempdir::TempDir;

use errors::*;
use {fs, io, parse, rustc, toml, xargo};
use rustc::{Channel, VersionMeta};
use {CommandExt, Target};

pub struct Crate {
    td: TempDir,
    triple: String,
}

impl Crate {
    pub fn build(rust_src: &Path,
                 target: &Target,
                 profile: &Option<toml::Value>,
                 verbose: bool)
                 -> Result<Self> {
        const CARGO_TOML: &'static str = r#"
[package]
authors = ["The Rust Project Developers"]
name = "sysroot"
version = "0.0.0"

[dependencies]
"#;

        let triple = target.triple();
        let sysroot = Crate {
            td: try!(TempDir::new("xargo")
                .chain_err(|| "couldn't create a temporary directory")),
            triple: triple.to_owned(),
        };
        let crate_dir = &sysroot.crate_dir().to_owned();

        let deps = if try!(target.has_atomics()) {
            vec!["alloc", "collections", "core", "rand", "rustc_unicode"]
        } else {
            vec!["core", "rand", "rustc_unicode"]
        };
        let mut toml = CARGO_TOML.to_owned();

        for dep in &deps {
            toml.push_str(&format!("{} = {{ path = \"{}\" }}\n",
                                   dep,
                                   rust_src.join(format!("src/lib{}", dep))
                                       .display()));
        }

        if let Some(profile) = profile.as_ref() {
            toml.push_str(&profile.to_string());
        }

        try!(io::write(&crate_dir.join("Cargo.toml"), &toml));
        try!(fs::mkdir(&crate_dir.join("src")));
        try!(io::write(&crate_dir.join("src/lib.rs"), ""));

        let cargo = || {
            let mut cmd = Command::new("cargo");
            cmd.env_remove("CARGO_TARGET_DIR");
            cmd.args(&["build", "--release", "--manifest-path"])
                .arg(crate_dir.join("Cargo.toml"));

            cmd.arg("--target");
            match *target {
                Target::BuiltIn { ref triple } |
                Target::Custom { ref triple, .. } => {
                    cmd.arg(triple);
                }
                Target::Path { ref json, .. } => {
                    cmd.arg(json);
                }
            }

            if verbose {
                cmd.arg("-v");
            }

            cmd
        };

        for dep in &deps {
            try!(cargo().arg("-p").arg(dep).run_or_error());
        }

        Ok(sysroot)
    }

    fn crate_dir(&self) -> &Path {
        self.td.path()
    }

    pub fn deps_dir(&self) -> PathBuf {
        self.td.path().join("target").join(&self.triple).join("release/deps")
    }
}

pub fn update(target: &Target, verbose: bool) -> Result<()> {
    let meta = try!(rustc::meta());

    if meta.channel != Channel::Nightly {
        try!(Err("Xargo requires the nightly channel. Run `rustup default \
                  nightly` or similar"))
    }

    try!(update_target_sysroot(target, &meta, verbose));
    try!(update_host_sysroot(&meta));

    Ok(())
}

fn update_target_sysroot(target: &Target,
                         meta: &VersionMeta,
                         verbose: bool)
                         -> Result<()> {
    // The hash is a digest of the following elements:
    // - RUSTFLAGS / build.rustflags / target.*.rustflags
    // - The [profile] in Cargo.toml
    // - The contents of the target specification file
    // - `rustc` version
    let hasher = &mut SipHasher::new();

    for flag in try!(rustc::flags(target, "rustflags")) {
        flag.hash(hasher);
    }

    let profile = try!(parse::profile_in_cargo_toml());
    if let Some(profile) = profile.as_ref() {
        profile.to_string().hash(hasher);
    }

    try!(target.hash(hasher));

    if let Some(commit_hash) = meta.commit_hash.as_ref() {
        commit_hash.hash(hasher);
    } else {
        meta.semver.hash(hasher);
    }

    let new_hash = hasher.finish();

    let triple = target.triple();
    let lock = try!(xargo::lock_rw(triple));
    let old_hash = try!(io::read_hash(&lock));

    if old_hash != Some(new_hash) {
        let rust_src = try!(rustc::rust_src());

        let sysroot = try!(Crate::build(&rust_src, target, &profile, verbose));

        try!(fs::remove_siblings(&lock));

        let dst = lock.parent().join("lib");
        try!(fs::cp_r(&sysroot.deps_dir(), &dst));

        try!(io::write_hash(&lock, new_hash));
    }

    Ok(())
}

fn update_host_sysroot(meta: &VersionMeta) -> Result<()> {
    let host = &meta.host;
    let lock = try!(xargo::lock_rw(host));

    let hasher = &mut SipHasher::new();
    host.hash(hasher);

    let new_hash = hasher.finish();
    let old_hash = try!(io::read_hash(&lock));

    if old_hash != Some(new_hash) {
        try!(fs::remove_siblings(&lock));

        let src =
            try!(rustc::sysroot()).join("lib/rustlib").join(host).join("lib");
        let dst = lock.parent().join("lib");
        try!(fs::cp_r(&src, &dst));

        try!(io::write_hash(&lock, new_hash));
    }

    Ok(())
}
