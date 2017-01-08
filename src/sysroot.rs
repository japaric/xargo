use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::fmt;

use rustc_version::VersionMeta;
use tempdir::TempDir;
use toml::Value;

use CompilationMode;
use cargo::{Root, Rustflags};
use errors::*;
use extensions::CommandExt;
use rustc::{Src, Sysroot};
use util;
use xargo::Home;
use {cargo, xargo};

#[cfg(feature = "dev")]
fn profile() -> &'static str {
    "debug"
}

#[cfg(not(feature = "dev"))]
fn profile() -> &'static str {
    "release"
}

fn build(cmode: &CompilationMode,
         deps: &Dependencies,
         ctoml: &cargo::Toml,
         home: &Home,
         rustflags: &Rustflags,
         hash: u64,
         verbose: bool)
         -> Result<()> {
    const TOML: &'static str = r#"
[package]
authors = ["The Rust Project Developers"]
name = "sysroot"
version = "0.0.0"
"#;

    let td = TempDir::new("xargo")
        .chain_err(|| "couldn't create a temporary directory")?;
    let td = td.path();

    let mut stoml = TOML.to_owned();
    stoml.push_str(&deps.to_string());

    if let Some(profile) = ctoml.profile() {
        stoml.push_str(&profile.to_string())
    }

    util::write(&td.join("Cargo.toml"), &stoml)?;
    util::mkdir(&td.join("src"))?;
    util::write(&td.join("src/lib.rs"), "")?;

    let cargo = || {
        let mut cmd = Command::new("cargo");
        cmd.env("RUSTFLAGS", rustflags.to_string());
        cmd.env_remove("CARGO_TARGET_DIR");
        cmd.arg("build");

        match () {
            #[cfg(feature = "dev")]
            () => {}
            #[cfg(not(feature = "dev"))]
            () => {
                cmd.arg("--release");
            }
        }
        cmd.arg("--manifest-path");
        cmd.arg(td.join("Cargo.toml"));
        cmd.args(&["--target", cmode.triple()]);

        if verbose {
            cmd.arg("-v");
        }

        cmd
    };

    for krate in deps.crates() {
        cargo().arg("-p").arg(krate).run(verbose)?;
    }

    // Copy artifacts to Xargo sysroot
    let rustlib = home.lock_rw(cmode.triple())?;
    rustlib.remove_siblings()
        .chain_err(|| format!("couldn't clear {}", rustlib.path().display()))?;
    let dst = rustlib.parent().join("lib");
    util::mkdir(&dst)?;
    util::cp_r(&td.join("target")
                   .join(cmode.triple())
                   .join(profile())
                   .join("deps"),
               &dst)?;

    // Create hash file
    util::write(&rustlib.parent().join(".hash"), &hash.to_string())?;

    Ok(())
}

fn old_hash(cmode: &CompilationMode, home: &Home) -> Result<Option<u64>> {
    // FIXME this should be `lock_ro`
    let lock = home.lock_rw(cmode.triple())?;
    let hfile = lock.parent().join(".hash");

    if hfile.exists() {
        Ok(util::read(&hfile)?.parse().ok())
    } else {
        Ok(None)
    }
}

/// Computes the hash of the would-be target sysroot
///
/// This information is used to compute the hash
///
/// - Dependencies in `Xargo.toml` for a specific target
/// - RUSTFLAGS / build.rustflags / target.*.rustflags
/// - The target specification file, is any
/// - `[profile.release]` in `Cargo.toml`
/// - `rustc` commit hash
fn hash(cmode: &CompilationMode,
        dependencies: &Dependencies,
        rustflags: &Rustflags,
        ctoml: &cargo::Toml,
        meta: &VersionMeta)
        -> Result<u64> {
    let mut hasher = DefaultHasher::new();

    dependencies.hash(&mut hasher);

    rustflags.hash(&mut hasher);

    cmode.hash(&mut hasher)?;

    if let Some(profile) = ctoml.profile() {
        profile.hash(&mut hasher);
    }

    if let Some(ref hash) = meta.commit_hash {
        hash.hash(&mut hasher);
    }

    Ok(hasher.finish())
}

pub fn update(cmode: &CompilationMode,
              home: &Home,
              root: &Root,
              rustflags: &Rustflags,
              meta: &VersionMeta,
              src: &Src,
              sysroot: &Sysroot,
              verbose: bool)
              -> Result<()> {
    let ctoml = cargo::toml(root)?;
    let xtoml = xargo::toml(root)?;

    let deps = Dependencies::from(xtoml.as_ref(), cmode.triple(), &src)?;

    let hash = hash(cmode, &deps, rustflags, &ctoml, meta)?;

    if old_hash(cmode, home)? != Some(hash) {
        build(cmode, &deps, &ctoml, home, rustflags, hash, verbose)?;
    }

    // copy host artifacts into the sysroot, if necessary
    if cmode.is_native() {
        return Ok(());
    }

    let lock = home.lock_rw(&meta.host)?;
    let hfile = lock.parent().join(".hash");

    let hash = meta.commit_hash.as_ref().map(|s| &**s).unwrap_or("");
    if hfile.exists() {
        if util::read(&hfile)? == hash {
            return Ok(());
        }
    }

    lock.remove_siblings()
        .chain_err(|| format!("couldn't clear {}", lock.path().display()))?;
    let dst = lock.parent().join("lib");
    util::mkdir(&dst)?;
    util::cp_r(&sysroot.path()
                   .join("lib/rustlib")
                   .join(&meta.host)
                   .join("lib"),
               &dst)?;

    util::write(&hfile, hash)?;

    Ok(())
}

/// Sysroot dependencies for a particular target
pub struct Dependencies {
    crates: Vec<String>,
    table: Value,
}

impl Dependencies {
    fn from(toml: Option<&xargo::Toml>,
            target: &str,
            src: &Src)
            -> Result<Self> {
        let mut deps =
            match (toml.and_then(|t| t.dependencies()),
                   toml.and_then(|t| t.target_dependencies(target))) {
                (Some(value), Some(tvalue)) => {
                    let mut deps = value.as_table()
                        .cloned()
                        .ok_or_else(|| {
                            format!("Xargo.toml: `dependencies` must be a \
                                     table")
                        })?;

                    let more_deps = tvalue.as_table()
                        .ok_or_else(|| {
                            format!("Xargo.toml: `target.{}.dependencies` \
                                     must be a table",
                                    target)
                        })?;
                    for (k, v) in more_deps {
                        if deps.insert(k.to_owned(), v.clone()).is_some() {
                            Err(format!("found duplicate dependency name \
                                         {}, but all dependencies must have \
                                         a unique name",
                                        k))?
                        }
                    }

                    deps
                }
                (Some(value), None) |
                (None, Some(value)) => {
                    if let Some(table) = value.as_table() {
                        table.clone()
                    } else {
                        Err(format!("Xargo.toml: target.{}.dependencies \
                                     must be a table",
                                    target))?
                    }
                }
                (None, None) => {
                    // If no dependencies were listed, we assume `core` as the
                    // only dependency
                    let mut t = BTreeMap::new();
                    t.insert("core".to_owned(), Value::Table(BTreeMap::new()));
                    t
                }
            };

        let mut crates = vec![];
        for (k, v) in deps.iter_mut() {
            crates.push(k.clone());

            let path =
                src.path().join(format!("lib{}", k)).display().to_string();

            if let Value::Table(ref mut map) = *v {
                map.insert("path".to_owned(), Value::String(path));
            } else {
                Err(format!("Xargo.toml: target.{}.dependencies.{} must be \
                             a table",
                            target,
                            k))?
            }
        }

        let mut map = BTreeMap::new();
        map.insert("dependencies".to_owned(), Value::Table(deps));

        Ok(Dependencies {
            crates: crates,
            table: Value::Table(map),
        })
    }

    fn crates(&self) -> &[String] {
        &self.crates
    }

    fn hash<H>(&self, hasher: &mut H)
        where H: Hasher
    {
        self.table.to_string().hash(hasher);
    }
}

impl Display for Dependencies {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.table, f)
    }
}
