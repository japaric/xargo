use std::hash::{Hash, Hasher, SipHasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use tempdir::TempDir;

use errors::*;
use rustc::{Channel, VersionMeta};
use {CommandExt, Target};
use {dag, fs, io, parse, rustc, toml, xargo};

/// A cargo create used to compile the sysroot
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

        let mut toml = CARGO_TOML.to_owned();

        toml.push_str(&format!("std = {{ path = '{}' }}\n",
                               rust_src.join("src/libstd").display()));

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

        let dg = try!(dag::build(rust_src));

        try!(dg.compile(|pkg| {
            cargo()
                .arg("-p")
                .arg(pkg)
                .run_and_get_status()
                .map(|es| es.success())
        }));

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

    if meta.channel != Channel::Nightly && meta.channel != Channel::Dev {
        try!(Err("Xargo requires the nightly channel. Run `rustup default \
                  nightly` or similar"))
    }

    try!(update_target_sysroot(target, &meta, verbose));
    try!(update_host_sysroot(&meta));

    Ok(())
}

/// Removes the profile.*.lto sections from a Cargo.toml
///
/// Returns `None` if the Cargo.toml becomes empty after pruning it
fn prune_cargo_toml(value: &toml::Value) -> Option<toml::Value> {
    let mut value = value.clone();

    let mut empty_profile_section = false;
    if let Some(&mut toml::Value::Table(ref mut profiles)) =
        value.lookup_mut("profile") {
        let mut gc_list = vec![];
        for (profile, options) in profiles.iter_mut() {
            if let toml::Value::Table(ref mut options) = *options {
                options.remove("lto");

                if options.is_empty() {
                    gc_list.push(profile.to_owned());
                }
            }
        }

        for profile in gc_list {
            profiles.remove(&profile[..]);
        }

        if profiles.is_empty() {
            empty_profile_section = true;
        }
    }

    let mut empty_cargo_toml = false;
    if empty_profile_section {
        if let toml::Value::Table(ref mut table) = value {
            table.remove("profile");

            if table.is_empty() {
                empty_cargo_toml = true;
            }
        }
    }

    if empty_cargo_toml { None } else { Some(value) }
}

fn prune_rustflags(flags: Vec<String>) -> Vec<String> {
    let mut pruned_flags = vec![];
    let mut flags = flags.into_iter();

    while let Some(flag) = flags.next() {
        if flag == "-C" {
            if let Some(next_flag) = flags.next() {
                if next_flag.starts_with("link-arg") {
                    // drop
                } else {
                    pruned_flags.push(flag);
                    pruned_flags.push(next_flag);
                }
            }
        } else {
            pruned_flags.push(flag);
        }
    }

    pruned_flags
}

fn update_target_sysroot(target: &Target,
                         meta: &VersionMeta,
                         verbose: bool)
                         -> Result<()> {
    // The hash is a digest of the following elements:
    // - RUSTFLAGS / build.rustflags / target.*.rustflags minus linker flags
    // - The [profile] in Cargo.toml minus its profile.*.lto sections
    // - The contents of the target specification file
    // - `rustc` version
    let hasher = &mut SipHasher::new();

    for flag in prune_rustflags(try!(rustc::flags(target, "rustflags"))) {
        flag.hash(hasher);
    }

    let profile = try!(parse::profile_in_cargo_toml());
    if let Some(profile) = profile.as_ref().and_then(prune_cargo_toml) {
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
