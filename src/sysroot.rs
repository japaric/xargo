use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

use rustc_version::VersionMeta;
use tempdir::TempDir;
use toml::{Table, Value};

use CompilationMode;
use cargo::{Root, Rustflags};
use errors::*;
use extensions::CommandExt;
use rustc::{Src, Sysroot, Target};
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

fn build(
    cmode: &CompilationMode,
    blueprint: Blueprint,
    ctoml: &cargo::Toml,
    home: &Home,
    rustflags: &Rustflags,
    sysroot: &Sysroot,
    hash: u64,
    verbose: bool,
) -> Result<()> {
    const TOML: &'static str = r#"
[package]
authors = ["The Rust Project Developers"]
name = "sysroot"
version = "0.0.0"
"#;

    let rustlib = home.lock_rw(cmode.triple())?;
    rustlib
        .remove_siblings()
        .chain_err(|| format!("couldn't clear {}", rustlib.path().display()))?;
    let dst = rustlib.parent().join("lib");
    util::mkdir(&dst)?;

    if cmode.triple().contains("pc-windows-gnu") {
        let src = &sysroot
            .path()
            .join("lib")
            .join("rustlib")
            .join(cmode.triple())
            .join("lib");

        // These are required for linking executables/dlls
        for file in ["rsbegin.o", "rsend.o", "crt2.o", "dllcrt2.o"].iter() {
            let file_src = src.join(file);
            let file_dst = dst.join(file);
            fs::copy(&file_src, &file_dst).chain_err(|| {
                format!(
                    "couldn't copy {} to {}",
                    file_src.display(),
                    file_dst.display()
                )
            })?;
        }
    }

    for (_, stage) in blueprint.stages {
        let td = TempDir::new("xargo").chain_err(|| "couldn't create a temporary directory")?;
        let tdp;
        let td = if env::var_os("XARGO_KEEP_TEMP").is_some() {
            tdp = td.into_path();
            &tdp
        } else {
            td.path()
        };

        let mut stoml = TOML.to_owned();
        {
            let mut map = Table::new();

            map.insert("dependencies".to_owned(), Value::Table(stage.dependencies));
            map.insert("patch".to_owned(), Value::Table(stage.patch));

            stoml.push_str(&Value::Table(map).to_string());
        }

        if let Some(profile) = ctoml.profile() {
            stoml.push_str(&profile.to_string())
        }

        util::write(&td.join("Cargo.toml"), &stoml)?;
        util::mkdir(&td.join("src"))?;
        util::write(&td.join("src/lib.rs"), "")?;

        let cargo = || {
            let mut cmd = Command::new("cargo");
            let mut flags = rustflags.for_xargo(home);
            flags.push_str(" -Z force-unstable-if-unmarked");
            if verbose {
                writeln!(io::stderr(), "+ RUSTFLAGS={:?}", flags).ok();
            }
            cmd.env("RUSTFLAGS", flags);
            cmd.env_remove("CARGO_TARGET_DIR");

            // As of rust-lang/cargo#4788 Cargo invokes rustc with a changed "current directory" so
            // we can't assume that such directory will be the same as the directory from which
            // Xargo was invoked. This is specially true when compiling the sysroot as the std
            // source is provided as a workspace and Cargo will change the current directory to the
            // root of the workspace when building one. To ensure rustc finds a target specification
            // file stored in the current directory we'll set `RUST_TARGET_PATH`  to the current
            // directory.
            if env::var_os("RUST_TARGET_PATH").is_none() {
                if let CompilationMode::Cross(ref target) = *cmode {
                    if let Target::Custom { ref json, .. } = *target {
                        cmd.env("RUST_TARGET_PATH", json.parent().unwrap());
                    }
                }
            }

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

        for krate in stage.crates {
            cargo().arg("-p").arg(krate).run(verbose)?;
        }

        // Copy artifacts to Xargo sysroot
        util::cp_r(
            &td.join("target")
                .join(cmode.triple())
                .join(profile())
                .join("deps"),
            &dst,
        )?;
    }

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
fn hash(
    cmode: &CompilationMode,
    blueprint: &Blueprint,
    rustflags: &Rustflags,
    ctoml: &cargo::Toml,
    meta: &VersionMeta,
) -> Result<u64> {
    let mut hasher = DefaultHasher::new();

    blueprint.hash(&mut hasher);

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

pub fn update(
    cmode: &CompilationMode,
    home: &Home,
    root: &Root,
    rustflags: &Rustflags,
    meta: &VersionMeta,
    src: &Src,
    sysroot: &Sysroot,
    verbose: bool,
) -> Result<()> {
    let ctoml = cargo::toml(root)?;
    let xtoml = xargo::toml(root)?;

    let blueprint = Blueprint::from(xtoml.as_ref(), cmode.triple(), root, &src)?;

    let hash = hash(cmode, &blueprint, rustflags, &ctoml, meta)?;

    if old_hash(cmode, home)? != Some(hash) {
        build(
            cmode,
            blueprint,
            &ctoml,
            home,
            rustflags,
            sysroot,
            hash,
            verbose,
        )?;
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
    util::cp_r(
        &sysroot
            .path()
            .join("lib/rustlib")
            .join(&meta.host)
            .join("lib"),
        &dst,
    )?;

    let bin_src = sysroot.path().join("lib/rustlib").join(&meta.host).join("bin");
    // copy the Rust linker if it exists
    if bin_src.exists() {
        let bin_dst = lock.parent().join("bin");
        util::mkdir(&bin_dst)?;
        util::cp_r(&bin_src, &bin_dst)?;
    }

    util::write(&hfile, hash)?;

    Ok(())
}

/// Per stage dependencies
#[derive(Debug)]
pub struct Stage {
    crates: Vec<String>,
    dependencies: Table,
    patch: Table,
}

/// A sysroot that will be built in "stages"
#[derive(Debug)]
pub struct Blueprint {
    stages: BTreeMap<i64, Stage>,
}

trait AsTableMut {
    fn as_table_mut<F, R>(&mut self, on_error_path: F) -> Result<&mut Table>
    where
        F: FnOnce() -> R,
        R: ::std::fmt::Display;
}

impl AsTableMut for Value {
    /// If the `self` is a Value::Table, return `Ok` with mutable reference to
    /// the contained table. If it's not return `Err` with an error message.
    /// The result of `on_error_path` will be inserted in the error message and
    /// should indicate the TOML path of `self`.
    fn as_table_mut<F, R>(&mut self, on_error_path: F) -> Result<&mut Table>
    where
        F: FnOnce() -> R,
        R: ::std::fmt::Display,
    {
        match self {
            Value::Table(table) => Ok(table),
            _ => Err(format!("Xargo.toml: `{}` must be a table", on_error_path()).into()),
        }
    }
}

impl Blueprint {
    fn new() -> Self {
        Blueprint {
            stages: BTreeMap::new(),
        }
    }

    fn from(toml: Option<&xargo::Toml>, target: &str, root: &Root, src: &Src) -> Result<Self> {
        fn make_path_absolute<F, R>(
            crate_spec: &mut toml::Table,
            root: &Root,
            on_error_path: F,
        ) -> Result<()>
        where
            F: FnOnce() -> R,
            R: ::std::fmt::Display,
        {
            if let Some(path) = crate_spec.get_mut("path") {
                let p = PathBuf::from(
                    path.as_str()
                        .ok_or_else(|| format!("`{}.path` must be a string", on_error_path()))?,
                );

                if !p.is_absolute() {
                    *path = Value::String(
                        root.path()
                            .join(&p)
                            .canonicalize()
                            .chain_err(|| format!("couldn't canonicalize {}", p.display()))?
                            .display()
                            .to_string(),
                    );
                }
            }
            Ok(())
        }

        let mut patch = match toml.and_then(xargo::Toml::patch) {
            Some(value) => value
                .as_table()
                .cloned()
                .ok_or_else(|| format!("Xargo.toml: `patch` must be a table"))?,
            None => Table::new()
        };

        for (k1, v) in patch.iter_mut() {
            for (k2, v) in v.as_table_mut(|| format!("patch.{}", k1))?.iter_mut() {
                let krate = v.as_table_mut(|| format!("patch.{}.{}", k1, k2))?;

                make_path_absolute(krate, root, || format!("patch.{}.{}", k1, k2))?;
            }
        }

        let rustc_std_workspace_core = src.path().join("tools/rustc-std-workspace-core");
        if rustc_std_workspace_core.exists() {
            // add rustc_std_workspace_core to patch section (if not specified)
            fn table_entry<'a>(table: &'a mut Table, key: &str) -> Result<&'a mut Table> {
                table
                    .entry(key.into())
                    .or_insert_with(|| Value::Table(Table::new()))
                    .as_table_mut(|| key)
            }

            let mut crates_io = table_entry(&mut patch, "crates-io")?;
            if !crates_io.contains_key("rustc-std-workspace-core") {
                table_entry(&mut crates_io, "rustc-std-workspace-core")?.insert(
                    "path".into(),
                    Value::String(rustc_std_workspace_core.display().to_string()),
                );
            }
        }

        let deps = match (
            toml.and_then(|t| t.dependencies()),
            toml.and_then(|t| t.target_dependencies(target)),
        ) {
            (Some(value), Some(tvalue)) => {
                let mut deps = value
                    .as_table()
                    .cloned()
                    .ok_or_else(|| format!("Xargo.toml: `dependencies` must be a table"))?;

                let more_deps = tvalue.as_table().ok_or_else(|| {
                    format!(
                        "Xargo.toml: `target.{}.dependencies` must be \
                         a table",
                        target
                    )
                })?;
                for (k, v) in more_deps {
                    if deps.insert(k.to_owned(), v.clone()).is_some() {
                        Err(format!(
                            "found duplicate dependency name {}, \
                             but all dependencies must have a \
                             unique name",
                            k
                        ))?
                    }
                }

                deps
            }
            (Some(value), None) | (None, Some(value)) => if let Some(table) = value.as_table() {
                table.clone()
            } else {
                Err(format!(
                    "Xargo.toml: target.{}.dependencies must be \
                     a table",
                    target
                ))?
            },
            (None, None) => {
                // If no dependencies were listed, we assume `core` and `compiler_builtins` as the
                // dependencies
                let mut t = BTreeMap::new();
                let mut core = BTreeMap::new();
                core.insert("stage".to_owned(), Value::Integer(0));
                t.insert("core".to_owned(), Value::Table(core));
                let mut cb = BTreeMap::new();
                cb.insert(
                    "features".to_owned(),
                    Value::Array(vec![Value::String("mem".to_owned())]),
                );
                cb.insert("stage".to_owned(), Value::Integer(1));
                t.insert(
                    "compiler_builtins".to_owned(),
                    Value::Table(cb),
                );
                t
            }
        };

        let mut blueprint = Blueprint::new();
        for (k, v) in deps {
            if let Value::Table(mut map) = v {
                let stage = if let Some(value) = map.remove("stage") {
                    value
                        .as_integer()
                        .ok_or_else(|| format!("dependencies.{}.stage must be an integer", k))?
                } else {
                    0
                };

                make_path_absolute(&mut map, root, || format!("dependencies.{}", k))?;

                if !map.contains_key("path") && !map.contains_key("git") {
                    // No path and no git given.  This might be in the sysroot, but if we don't find it there we assume it comes from crates.io.
                    let path = src.path().join(format!("lib{}", k));
                    if path.exists() {
                        map.insert("path".to_owned(), Value::String(path.display().to_string()));
                    }
                }

                blueprint.push(stage, k, map, &patch);
            } else {
                Err(format!(
                    "Xargo.toml: target.{}.dependencies.{} must be \
                     a table",
                    target, k
                ))?
            }
        }

        Ok(blueprint)
    }

    fn push(&mut self, stage: i64, krate: String, toml: Table, patch: &Table) {
        let stage = self.stages.entry(stage).or_insert_with(|| Stage {
            crates: vec![],
            dependencies: Table::new(),
            patch: patch.clone(),
        });

        stage.dependencies.insert(krate.clone(), Value::Table(toml));
        stage.crates.push(krate);
    }

    fn hash<H>(&self, hasher: &mut H)
    where
        H: Hasher,
    {
        for stage in self.stages.values() {
            for (k, v) in stage.dependencies.iter() {
                k.hash(hasher);
                v.to_string().hash(hasher);
            }
        }
    }
}
