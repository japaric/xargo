use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::{env, fs};

use rustc_version::VersionMeta;
use tempdir::TempDir;
use toml::{value::Table, Value, map::Map};

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
    ctoml: &Option<cargo::Toml>,
    home: &Home,
    rustflags: &Rustflags,
    src: &Src,
    sysroot: &Sysroot,
    hash: u64,
    verbose: bool,
    message_format: Option<&str>,
    cargo_mode: XargoMode,
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

    if cmode.triple().contains("pc-windows-gnu") && cargo_mode == XargoMode::Build {
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

        if let Some(ctoml) = ctoml {
            if let Some(profile) = ctoml.profile() {
                stoml.push_str(&profile.to_string())
            }
        }

        // rust-src comes with a lockfile for libstd. Use it.
        let src_parent = src.path().parent().map(Path::to_path_buf).unwrap_or_else(|| src.path().join(".."));
        let lockfile = src_parent.join("Cargo.lock");
        let target_lockfile = td.join("Cargo.lock");
        fs::copy(lockfile, &target_lockfile).chain_err(|| "Cargo.lock file is missing from source dir")?;

        let mut perms = fs::metadata(&target_lockfile)
            .chain_err(|| "Cargo.lock file is missing from target dir")?
            .permissions();
        perms.set_readonly(false);
        fs::set_permissions(&target_lockfile, perms)
            .chain_err(|| "Cargo.lock file is missing from target dir")?;

        util::write(&td.join("Cargo.toml"), &stoml)?;
        util::mkdir(&td.join("src"))?;
        util::write(&td.join("src").join("lib.rs"), "")?;

        let cargo = || {
            let mut cmd = cargo::command();
            let mut flags = rustflags.for_xargo(home);
            flags.push_str(" -Z force-unstable-if-unmarked");
            if verbose {
                writeln!(io::stderr(), "+ RUSTFLAGS={:?}", flags).ok();
            }
            cmd.env("RUSTFLAGS", flags);

            // Since we currently don't want to respect `.cargo/config` or `CARGO_TARGET_DIR`,
            // we need to force the target directory to match the `cp_r` below.
            cmd.env("CARGO_TARGET_DIR", td.join("target"));

            // Workaround #261.
            //
            // If a crate is shared between the sysroot and a binary, we might
            // end up with conflicting symbols. This is because both versions
            // of the crate would get linked, and their metadata hash would be
            // exactly the same.
            //
            // To avoid this, we need to inject some data that modifies the
            // metadata hash. Fortunately, cargo already has a mechanism for
            // this, the __CARGO_DEFAULT_LIB_METADATA environment variable.
            // Unsurprisingly, rust's bootstrap (which has basically the same
            // role as xargo of building the libstd) makes use of this
            // environment variable to avoid exactly this problem. See here:
            // https://github.com/rust-lang/rust/blob/73369f32621f6a844a80a8513ae3ded901e4a406/src/bootstrap/builder.rs#L876
            //
            // This relies on an **unstable cargo feature** that isn't meant to
            // be used outside the bootstrap. This is explicitly stated in
            // cargo's source:
            // https://github.com/rust-lang/cargo/blob/14654f38d0819c47d7a605d6f1797ffbcdc65000/src/cargo/core/compiler/context/compilation_files.rs#L496
            // Unfortunately, I don't see any other way out. We need to have a
            // way to modify the crate's hash, and from the outside this is the
            // only way to do so.
            cmd.env("__CARGO_DEFAULT_LIB_METADATA", "xargo");

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

            match cargo_mode {
                XargoMode::Build => cmd.arg("build"),
                XargoMode::Check => cmd.arg("check")
            };

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
            if let Some(format) = message_format {
                cmd.args(&["--message-format", format]);
            }

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
    ctoml: &Option<cargo::Toml>,
    meta: &VersionMeta,
) -> Result<u64> {
    let mut hasher = DefaultHasher::new();

    blueprint.hash(&mut hasher);

    rustflags.hash(&mut hasher);

    cmode.hash(&mut hasher)?;

    if let Some(ctoml) = ctoml {
        if let Some(profile) = ctoml.profile() {
            profile.hash(&mut hasher);
        }
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
    message_format: Option<&str>,
    cargo_mode: XargoMode,
) -> Result<()> {
    let ctoml = match cargo_mode {
        XargoMode::Build => Some(cargo::toml(root)?),
        XargoMode::Check => {
            if root.path().join("Cargo.toml").exists() {
                Some(cargo::toml(root)?)
            } else {
                None
            }
        }
    };

    let (xtoml_parent, xtoml) = xargo::toml(root)?;

    // As paths in the 'Xargo.toml' can be relative to the directory containing
    // the 'Xargo.toml', we need to pass the path containing it to the
    // Blueprint. Otherwise, if no 'Xargo.toml' is found, we use the regular
    // root path.
    let base_path: &Path = xtoml_parent.unwrap_or_else(|| root.path());

    let blueprint = Blueprint::from(xtoml.as_ref(), cmode.triple(), &base_path, &src)?;

    let hash = hash(cmode, &blueprint, rustflags, &ctoml, meta)?;

    if old_hash(cmode, home)? != Some(hash) {
        build(
            cmode,
            blueprint,
            &ctoml,
            home,
            rustflags,
            src,
            sysroot,
            hash,
            verbose,
            message_format,
            cargo_mode,
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
            .join("lib")
            .join("rustlib")
            .join(&meta.host)
            .join("lib"),
        &dst,
    )?;

    let bin_src = sysroot.path().join("lib").join("rustlib").join(&meta.host).join("bin");
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

/// Which mode to invoke `cargo` in when building the sysroot
/// Can be either `cargo build` or `cargo check`
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum XargoMode {
    Build,
    Check,
}

/// A sysroot that will be built in "stages"
#[derive(Debug)]
pub struct Blueprint {
    stages: BTreeMap<i64, Stage>,
}

trait AsTableMut {
    fn as_table_mut_or_err<F, R>(&mut self, on_error_path: F) -> Result<&mut Table>
    where
        F: FnOnce() -> R,
        R: ::std::fmt::Display;
}

impl AsTableMut for Value {
    /// If the `self` is a Value::Table, return `Ok` with mutable reference to
    /// the contained table. If it's not return `Err` with an error message.
    /// The result of `on_error_path` will be inserted in the error message and
    /// should indicate the TOML path of `self`.
    fn as_table_mut_or_err<F, R>(&mut self, on_error_path: F) -> Result<&mut Table>
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

    /// Add $CRATE to `patch` section, as needed to build libstd.
    fn add_patch(patch: &mut Table, src_path: &Path, crate_: &str) -> Result<()> {
        // Old sysroots have this in `src/tools/$CRATE`, new sysroots in `library/$CRATE`.
        let paths = [
            src_path.join(crate_),
            src_path.join("tools").join(crate_),
        ];
        if let Some(path) = paths.iter().find(|p| p.exists()) {
            // add crate to patch section (if not specified)
            fn table_entry<'a>(table: &'a mut Table, key: &str) -> Result<&'a mut Table> {
                table
                    .entry(key)
                    .or_insert_with(|| Value::Table(Table::new()))
                    .as_table_mut_or_err(|| key)
            }

            let crates_io = table_entry(patch, "crates-io")?;
            if !crates_io.contains_key(crate_) {
                table_entry(crates_io, crate_)?
                    .insert("path".into(), Value::String(path.display().to_string()));
            }
        }
        Ok(())
    }

    fn from(toml: Option<&xargo::Toml>, target: &str, base_path: &Path, src: &Src) -> Result<Self> {
        fn make_path_absolute<F, R>(
            crate_spec: &mut Table,
            base_path: &Path,
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
                        base_path
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

        // Compose patch section
        let mut patch = match toml.and_then(xargo::Toml::patch) {
            Some(value) => value
                .as_table()
                .cloned()
                .ok_or_else(|| format!("Xargo.toml: `patch` must be a table"))?,
            None => Table::new()
        };

        for (k1, v) in patch.iter_mut() {
            for (k2, v) in v.as_table_mut_or_err(|| format!("patch.{}", k1))?.iter_mut() {
                let krate = v.as_table_mut_or_err(|| format!("patch.{}.{}", k1, k2))?;

                make_path_absolute(krate, base_path, || format!("patch.{}.{}", k1, k2))?;
            }
        }

        Blueprint::add_patch(&mut patch, src.path(), "rustc-std-workspace-core")?;
        Blueprint::add_patch(&mut patch, src.path(), "rustc-std-workspace-alloc")?;
        Blueprint::add_patch(&mut patch, src.path(), "rustc-std-workspace-std")?;

        // Compose dependency sections
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
                let mut t = Map::new();
                let mut core = Map::new();
                core.insert("stage".to_owned(), Value::Integer(0));
                t.insert("core".to_owned(), Value::Table(core));
                let mut cb = Map::new();
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

                make_path_absolute(&mut map, base_path, || format!("dependencies.{}", k))?;

                if !map.contains_key("path") && !map.contains_key("git") {
                    // No path and no git given.  This might be in the sysroot, but if we don't find it there we assume it comes from crates.io.
                    // Current sysroots call it just "std" (etc), but older sysroots use "libstd" (etc),
                    // so we check both.
                    let paths = [
                        src.path().join(&k),
                        src.path().join(format!("lib{}", k)),
                    ];
                    if let Some(path) = paths.iter().find(|p| p.exists()) {
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
