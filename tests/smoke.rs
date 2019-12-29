#![cfg_attr(not(feature = "dev"), allow(dead_code))]
#![deny(warnings)]
#![feature(const_fn)]

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate lazy_static;
extern crate parking_lot;
extern crate rustc_version;
extern crate tempdir;
extern crate dirs;

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use parking_lot::{Mutex, MutexGuard};
use tempdir::TempDir;

use errors::*;

mod errors {
    #![allow(unused_doc_comments)]
    error_chain!();
}

macro_rules! run {
    () => {
        if let Err(e) = run() {
            panic!("{}", e)
        }
    }
}

// Returns Xargo's "home"
fn home() -> Result<PathBuf> {
    if let Some(h) = env::var_os("XARGO_HOME") {
        Ok(PathBuf::from(h))
    } else {
        Ok(
            dirs::home_dir()
                .ok_or_else(|| "couldn't find your home directory. Is $HOME set?")?
                .join(".xargo"),
        )
    }
}

fn cleanup(target: &str) -> Result<()> {
    let p = home()?.join("lib/rustlib").join(target);

    if p.exists() {
        fs::remove_dir_all(&p).chain_err(|| format!("couldn't clean sysroot for {}", target))
    } else {
        Ok(())
    }
}

fn exists(krate: &str, target: &str) -> Result<bool> {
    let p = home()?.join("lib/rustlib").join(target).join("lib");

    for e in fs::read_dir(&p).chain_err(|| format!("couldn't read the directory {}", p.display()))?
    {
        let e = e.chain_err(|| {
            format!(
                "error reading the contents of the directory {}",
                p.display()
            )
        })?;

        if e.file_name().to_string_lossy().contains(krate) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn host() -> String {
    rustc_version::version_meta().host
}

fn mkdir(path: &Path) -> Result<()> {
    fs::create_dir(path).chain_err(|| {
        format!("couldn't create the directory {}", path.display())
    })
}

fn sysroot_was_built(stderr: &str, target: &str) -> bool {
    stderr.lines().filter(|l| l.starts_with("+")).any(|l| {
        l.contains("cargo") && l.contains("build") && l.contains("--target") && l.contains(target)
            && l.contains("-p") && l.contains("core")
    })
}

fn write(path: &Path, append: bool, contents: &str) -> Result<()> {
    let p = path.display();
    let mut opts = OpenOptions::new();

    if append {
        opts.append(true);
    } else {
        opts.create(true);
        opts.truncate(true);
    }

    opts.write(true)
        .open(path)
        .chain_err(|| format!("couldn't open {}", p))?
        .write_all(contents.as_bytes())
        .chain_err(|| format!("couldn't write to {}", p))
}

fn xargo() -> Result<Command> {
    let mut p = env::current_exe().chain_err(|| "couldn't get path to current executable")?;
    p.pop();
    p.pop();
    p.push("xargo");
    Ok(Command::new(p))
}

fn xargo_check() -> Result<Command> {
    let mut p = env::current_exe().chain_err(|| "couldn't get path to current executable")?;
    p.pop();
    p.pop();
    p.push("xargo-check");
    Ok(Command::new(p))
}

trait CommandExt {
    fn run(&mut self) -> Result<()>;
    fn run_and_get_stderr(&mut self) -> Result<String>;
}

impl CommandExt for Command {
    fn run(&mut self) -> Result<()> {
        let status = self.status()
            .chain_err(|| format!("couldn't execute `{:?}`", self))?;

        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "`{:?}` failed with exit code: {:?}",
                self,
                status.code()
            ))?
        }
    }

    fn run_and_get_stderr(&mut self) -> Result<String> {
        let out = self.output()
            .chain_err(|| format!("couldn't execute `{:?}`", self))?;

        let stderr = String::from_utf8(out.stderr)
            .chain_err(|| format!("`{:?}` output was not UTF-8", self));

        if out.status.success() {
            stderr
        } else {
            match stderr {
                Ok(e) => print!("{}", e),
                Err(e) => print!("{}", e),
            }
            Err(format!(
                "`{:?}` failed with exit code: {:?}",
                self,
                out.status.code()
            ))?
        }
    }
}

struct Project {
    name: &'static str,
    td: TempDir,
}

/// Create a simple project
fn create_simple_project(project_path: &Path, name: &'static str, library_source: &'static str) -> Result<()> {
    xargo()?
        .args(&["init", "-q", "--lib", "--vcs", "none", "--name", name])
        .current_dir(project_path)
        .run()?;

   write(&project_path.join("src/lib.rs"), false, library_source)?;

   Ok(())
}

impl Project {
    /// Creates a new project with given name in a temporary directory.
    fn new(name: &'static str) -> Result<Self> {
        Self::new_in(std::env::temp_dir(), name)
    }

    /// Creates a new project with given name in a sub directory of `dir`.
    fn new_in(dir: PathBuf, name: &'static str) -> Result<Self> {
        const JSON: &'static str = r#"
{
    "arch": "arm",
    "data-layout": "e-m:e-p:32:32-i64:64-v128:64:128-a:0:32-n32-S64",
    "linker-flavor": "gcc",
    "llvm-target": "thumbv6m-none-eabi",
    "max-atomic-width": 0,
    "os": "none",
    "target-c-int-width": "32",
    "target-endian": "little",
    "target-pointer-width": "32"
}
"#;

        let td = TempDir::new_in(dir, "xargo").chain_err(|| "couldn't create a temporary directory")?;

        create_simple_project(td.path(), name, "#![no_std]")?;
        write(&td.path().join(format!("{}.json", name)), false, JSON)?;

        Ok(Project { name: name, td: td })
    }

    /// Calls `xargo build`
    fn build(&self, target: &str) -> Result<()> {
        // Be less verbose
        self.build_and_get_stderr(Some(target))?;
        Ok(())
    }

    /// Calls `xargo build` and collects STDERR
    fn build_and_get_stderr(&self, target: Option<&str>) -> Result<String> {
        let mut cmd = xargo()?;
        cmd.arg("build");

        if let Some(target) = target {
            cmd.args(&["--target", target]);
        }

        cmd.arg("-v")
            .current_dir(self.td.path())
            .run_and_get_stderr()
    }

    /// Appends a string to the project `Cargo.toml`
    fn cargo_toml(&self, contents: &str) -> Result<()> {
        write(&self.td.path().join("Cargo.toml"), true, contents)
    }

    /// Adds a `.cargo/config` to the project
    fn config(&self, contents: &str) -> Result<()> {
        mkdir(&self.td.path().join(".cargo"))?;

        write(&self.td.path().join(".cargo/config"), false, contents)
    }

    /// Calls `xargo doc`
    fn doc(&self, target: &str) -> Result<()> {
        xargo()?
            .args(&["doc", "--target", target])
            .current_dir(self.td.path())
            .run_and_get_stderr()?;
        Ok(())
    }


    /// Adds a `Xargo.toml` to the project
    fn xargo_toml(&self, toml: &str) -> Result<()> {
        write(&self.td.path().join("Xargo.toml"), false, toml)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        cleanup(self.name).unwrap()
    }
}

fn hcleanup(triple: &str) -> Result<()> {
    let p = home()?.join("HOST/lib/rustlib").join(triple);

    if p.exists() {
        fs::remove_dir_all(&p).chain_err(|| format!("couldn't clean sysroot for {}", triple))
    } else {
        Ok(())
    }
}

struct HProject {
    _guard: MutexGuard<'static, ()>,
    host: String,
    td: TempDir,
}

impl HProject {
    fn new(test: bool) -> Result<Self> {
        // There can only be one instance of this type at any point in time.
        // Needed to make sure we don't try to build multiple HOST libstds in parallel.
        lazy_static! {
            static ref ONCE: Mutex<()> = Mutex::new(());
        }

        let guard = ONCE.lock();

        let td = TempDir::new("xargo").chain_err(|| "couldn't create a temporary directory")?;

        xargo()?
            .args(&["init", "-q", "--lib", "--vcs", "none", "--name", "host"])
            .current_dir(td.path())
            .run()?;

        if test {
            write(
                &td.path().join("src/lib.rs"),
                false,
                "fn _f(_: Vec<std::fs::File>) {}",
            )?;
        } else {
            write(&td.path().join("src/lib.rs"), false, "#![no_std]")?;
        }

        Ok(HProject {
            _guard: guard,
            host: host(),
            td: td,
        })
    }

    fn build(&self, verb: &str) -> Result<()> {
        // Calling "run_and_get_stderr" to be less verbose
        xargo()?.arg(verb).current_dir(self.td.path()).run_and_get_stderr()?;
        Ok(())
    }

    /// Calls `xargo build` and collects STDERR
    fn build_and_get_stderr(&self) -> Result<String> {
        let mut cmd = xargo()?;
        cmd.arg("build");

        cmd.arg("-v")
            .current_dir(self.td.path())
            .run_and_get_stderr()
    }

    /// Adds a `Xargo.toml` to the project
    fn xargo_toml(&self, toml: &str) -> Result<()> {
        write(&self.td.path().join("Xargo.toml"), false, toml)
    }

    /// Runs `xargo-check` with the specified subcommand
    fn xargo_check_subcommand(&self, subcommand: Option<&str>) -> Result<String> {
        let mut cmd = xargo_check()?;
        if let Some(subcommand) = subcommand {
            cmd.args(&[subcommand]);
        }
        cmd
            .current_dir(self.td.path())
            .run_and_get_stderr()
    }

}

impl Drop for HProject {
    fn drop(&mut self) {
        hcleanup(&self.host).unwrap()
    }
}

/// Test vanilla `xargo build`
#[cfg(feature = "dev")]
#[test]
fn simple() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-simple-eabi";

        let project = Project::new(TARGET)?;
        project.build(TARGET)?;
        assert!(exists("core", TARGET)?);

        Ok(())
    }

    run!()
}

/// Test building a dependency specified as `target.{}.dependencies` in
/// ../Xargo.toml
#[cfg(feature = "dev")]
#[test]
fn target_dependencies() {
    fn run() -> Result<()> {
        // need this exact target name to get the right gcc flags
        const TARGET: &'static str = "thumbv7m-none-eabi";
        const STAGE1: &'static str = "stage1";

        let td = TempDir::new("xargo").chain_err(|| "couldn't create a temporary directory")?;
        let project = Project::new_in(td.path().to_path_buf(), TARGET)?;

        let stage1_path = td.path().join(STAGE1);

        mkdir(stage1_path.as_path())?;
        create_simple_project(stage1_path.as_path(), STAGE1, "#![no_std]")?;
        write(&td.path().join("Xargo.toml"), false,
            r#"
[target.thumbv7m-none-eabi.dependencies.alloc]

[target.thumbv7m-none-eabi.dependencies.stage1]
stage = 1
path = "stage1"
"#,
        )?;
        project.build(TARGET)?;
        assert!(exists("core", TARGET)?);
        assert!(exists("alloc", TARGET)?);
        assert!(exists("stage1", TARGET)?);
        Ok(())
    }

    run!()
}

/// Test building a dependency specified as `dependencies` in Xargo.toml
#[cfg(feature = "dev")]
#[test]
fn dependencies() {
    fn run() -> Result<()> {
        // need this exact target name to get the right gcc flags
        const TARGET: &'static str = "thumbv6m-none-eabi";

        let project = Project::new(TARGET)?;
        project.xargo_toml(
            r#"
[dependencies.alloc]
"#,
        )?;
        project.build(TARGET)?;
        assert!(exists("core", TARGET)?);
        assert!(exists("alloc", TARGET)?);

        Ok(())
    }

    run!()
}

/// Test `xargo doc`
#[cfg(feature = "dev")]
#[test]
fn doc() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-doc-eabi";

        let project = Project::new(TARGET)?;
        project.doc(TARGET)?;
        assert!(exists("core", TARGET)?);

        Ok(())
    }

    run!()
}

/// Check that calling `xargo build` a second time doesn't rebuild the sysroot
#[cfg(feature = "dev")]
#[test]
fn twice() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-twice-eabi";

        let project = Project::new(TARGET)?;
        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(!sysroot_was_built(&stderr, TARGET));

        Ok(())
    }

    run!()
}

/// Check that if `build.target` is set in `.cargo/config`, that target will be
/// used to build the sysroot
#[cfg(feature = "dev")]
#[test]
fn build_target() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-build_target-eabi";

        let project = Project::new(TARGET)?;
        project.config(
            r#"
[build]
target = "thumbv6m-build_target-eabi"
"#,
        )?;

        let stderr = project.build_and_get_stderr(None)?;

        assert!(sysroot_was_built(&stderr, TARGET));

        Ok(())
    }

    run!()
}

/// Check that `--target` overrides `build.target`
#[cfg(feature = "dev")]
#[test]
fn override_build_target() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-override_build_target-eabi";

        let project = Project::new(TARGET)?;
        project.config(
            r#"
[build]
target = "BAD"
"#,
        )?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        Ok(())
    }

    run!()
}

/// We shouldn't rebuild the sysroot if `profile.release.lto` changed
#[cfg(feature = "dev")]
#[test]
fn lto_changed() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-lto_changed-eabi";

        let project = Project::new(TARGET)?;
        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        project.cargo_toml(
            r#"
[profile.release]
lto = true
"#,
        )?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(!sysroot_was_built(&stderr, TARGET));

        Ok(())
    }

    run!()
}

/// Modifying RUSTFLAGS should trigger a rebuild of the sysroot
#[cfg(feature = "dev")]
#[test]
fn rustflags_changed() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-rustflags_changed-eabi";

        let project = Project::new(TARGET)?;
        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        project.config(
            r#"
[build]
rustflags = ["--cfg", "xargo"]
"#,
        )?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        Ok(())
    }

    run!()
}

/// Check that RUSTFLAGS are passed to all `rustc`s
#[cfg(feature = "dev")]
#[test]
fn rustflags() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-rustflags-eabi";

        let project = Project::new(TARGET)?;

        project.config(
            r#"
[build]
rustflags = ["--cfg", "xargo"]
"#,
        )?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(
            stderr
                .lines()
                .filter(|l| !l.starts_with("+") && l.contains("rustc") && !l.contains("rustc-std-workspace"))
                .all(|l| l.contains("--cfg") && l.contains("xargo")),
            "unexpected stderr:\n{}", stderr
        );

        Ok(())
    }

    run!()
}

/// Check that `-C panic=abort` is passed to `rustc` when `panic = "abort"` is
/// set in `profile.release`
#[cfg(not(feature = "dev"))]
#[test]
fn panic_abort() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-panic_abort-eabi";

        let project = Project::new(TARGET)?;

        project.cargo_toml(
            r#"
[profile.release]
panic = "abort"
"#,
        )?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(
            stderr
                .lines()
                .filter(|l| !l.starts_with("+") && l.contains("--release"))
                .all(|l| l.contains("-C") && l.contains("panic=abort"))
        );

        Ok(())
    }

    run!()
}

/// Check that adding linker arguments doesn't trigger a sysroot rebuild
#[cfg(feature = "dev")]
#[test]
fn link_arg() {
    fn run() -> Result<()> {
        const TARGET: &'static str = "thumbv6m-link_arg-eabi";

        let project = Project::new(TARGET)?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        project.config(
            r#"
[target.__link_arg]
rustflags = ["-C", "link-arg=-lfoo"]
"#,
        )?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(!sysroot_was_built(&stderr, TARGET));

        Ok(())
    }

    run!()
}

/// The sysroot should be rebuilt if the target specification changed
#[cfg(feature = "dev")]
#[test]
fn specification_changed() {
    fn run() -> Result<()> {
        const JSON: &'static str = r#"
{
    "arch": "arm",
    "data-layout": "e-m:e-p:32:32-i64:64-v128:64:128-a:0:32-n32-S64",
    "linker-flavor": "gcc",
    "llvm-target": "thumbv6m-none-eabi",
    "max-atomic-width": 0,
    "os": "none",
    "panic-strategy": "abort",
    "target-c-int-width": "32",
    "target-endian": "little",
    "target-pointer-width": "32"
}
"#;
        const TARGET: &'static str = "thumbv6m-specification_changed-eabi";

        let project = Project::new(TARGET)?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        write(
            &project.td.path().join("thumbv6m-specification_changed-eabi.json"),
            false,
            JSON,
        )?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        Ok(())
    }

    run!()
}

/// The sysroot should NOT be rebuilt if the target specification didn't really
/// changed, e.g. some fields were moved around
#[cfg(feature = "dev")]
#[test]
fn unchanged_specification() {
    fn run() -> Result<()> {
        const JSON: &'static str = r#"
{
    "arch": "arm",
    "data-layout": "e-m:e-p:32:32-i64:64-v128:64:128-a:0:32-n32-S64",
    "linker-flavor": "gcc",
    "llvm-target": "thumbv6m-none-eabi",
    "os": "none",
    "max-atomic-width": 0,
    "target-c-int-width": "32",
    "target-endian": "little",
    "target-pointer-width": "32"
}
"#;
        const TARGET: &'static str = "thumbv6m-unchanged_specification-eabi";

        let project = Project::new(TARGET)?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(sysroot_was_built(&stderr, TARGET));

        write(
            &project.td.path().join("thumbv6m-unchanged_specification-eabi.json"),
            false,
            JSON,
        )?;

        let stderr = project.build_and_get_stderr(Some(TARGET))?;

        assert!(!sysroot_was_built(&stderr, TARGET));

        Ok(())
    }

    run!()
}

/// Check that a sysroot is built for the host
#[cfg(feature = "dev")]
#[test]
fn host_once() {
    fn run() -> Result<()> {
        let target = host();
        let project = HProject::new(false)?;

        let stderr = project.build_and_get_stderr()?;

        assert!(sysroot_was_built(&stderr, &target));

        Ok(())
    }

    run!()
}

/// Check that the sysroot is not rebuilt when `xargo build` is called a second
/// time
#[cfg(feature = "dev")]
#[test]
fn host_twice() {
    fn run() -> Result<()> {
        let target = host();
        let project = HProject::new(false)?;

        let stderr = project.build_and_get_stderr()?;

        assert!(sysroot_was_built(&stderr, &target));

        let stderr = project.build_and_get_stderr()?;

        assert!(!sysroot_was_built(&stderr, &target));

        Ok(())
    }

    run!()
}

/// Check multi stage sysroot builds with `xargo test`
#[cfg(feature = "dev")]
#[test]
fn host_libtest() {
    fn run() -> Result<()> {
        let project = HProject::new(true)?;

        if std::env::var("TRAVIS_RUST_VERSION").ok().map_or(false,
            |var| var.starts_with("nightly-"))
        {
            // Testing an old version on CI, we need a different Xargo.toml.
            project.xargo_toml(
            "
[dependencies.std]
features = [\"panic_unwind\"]

[dependencies.test]
stage = 1
",
            )?;
        } else {
            project.xargo_toml(
                "
[dependencies.std]
features = [\"panic_unwind\"]

[dependencies.test]
",
            )?;
        }

        project.build("test")
    }

    run!()
}

/// Check multi stage sysroot builds with `xargo build`
#[cfg(feature = "dev")]
#[test]
fn host_liballoc() {
    fn run() -> Result<()> {
        let project = HProject::new(false)?;

        project.xargo_toml(
            "
[dependencies.core]
stage = 0

[dependencies.alloc]
stage = 1
",
        )?;

        project.build("build")
    }

    run!()
}

/// Test having a `[patch]` section.
/// The tag in the toml file needs to be updated any time the version of
/// cc used by rustc is updated.
#[cfg(feature = "dev")]
#[test]
fn host_patch() {
    fn run() -> Result<()> {
        let project = HProject::new(false)?;
        project.xargo_toml(
            r#"
[dependencies.std]
features = ["panic_unwind"]

[patch.crates-io.cc]
git = "https://github.com/alexcrichton/cc-rs"
tag = "1.0.25"
"#,
        )?;
        let stderr = project.build_and_get_stderr()?;

        assert!(stderr
            .lines()
            .any(|line| line.contains("Compiling cc ")
                && line.contains("https://github.com/alexcrichton/cc-rs")),
            "Looks like patching did not work. stderr:\n{}", stderr
        );

        Ok(())
    }

    // Only run this on pinned nightlies, to avoid having to update the version number all the time.
    let is_pinned = std::env::var("TRAVIS_RUST_VERSION").ok().map_or(false,
            |var| var.starts_with("nightly-"));
    if is_pinned {
        run!()
    }
}

#[cfg(feature = "dev")]
#[test]
fn cargo_check_check() {
    fn run() -> Result<()> {
        let project = HProject::new(false)?;
        project.xargo_check_subcommand(Some("check"))?;

        Ok(())
    }
    run!()
}

#[cfg(feature = "dev")]
#[test]
fn cargo_check_check_no_ctoml() {
    fn run() -> Result<()> {
        let project = HProject::new(false)?;
        // Make sure that 'Xargo.toml` exists
        project.xargo_toml("")?;
        std::fs::remove_file(project.td.path().join("Cargo.toml"))
            .chain_err(|| format!("Could not remove Cargo.toml"))?;

        let stderr = project.xargo_check_subcommand(None)?;
        assert!(stderr.contains("Checking core"));

        Ok(())
    }
    run!()
}
