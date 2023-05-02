use std::path::{Display, Path, PathBuf};
use std::process::ExitStatus;
use std::{env, mem};
use std::io::{self, Write};

use toml::Value;
use rustc_version::VersionMeta;

use CompilationMode;
use cargo::{Config, Root, Rustflags, Subcommand};
use cli::Args;
use errors::*;
use extensions::CommandExt;
use flock::{FileLock, Filesystem};
use {cargo, util};

pub fn run(
    args: &Args,
    cmode: &CompilationMode,
    rustflags: Rustflags,
    home: &Home,
    meta: &VersionMeta,
    config: Option<&Config>,
    verbose: bool,
) -> Result<ExitStatus> {
    let mut cmd = cargo::command();
    cmd.args(args.all());

    if args.subcommand() == Some(Subcommand::Doc) {
        cmd.env(
            "CARGO_ENCODED_RUSTDOCFLAGS",
            cargo::rustdocflags(config, cmode.triple())?.encode(home),
        );
    }

    if verbose {
        writeln!(io::stderr(), "+ RUSTFLAGS={}", rustflags).ok();
    }
    cmd.env("CARGO_ENCODED_RUSTFLAGS", rustflags.encode(home));

    let locks = (home.lock_ro(&meta.host), home.lock_ro(cmode.triple()));

    let status = cmd.run_and_get_status(verbose)?;

    mem::drop(locks);

    Ok(status)
}

pub struct Home {
    path: Filesystem,
}

impl Home {
    pub fn display(&self) -> Display {
        self.path.display()
    }

    fn path(&self, triple: &str) -> Filesystem {
        self.path.join("lib").join("rustlib").join(triple)
    }

    pub fn lock_ro(&self, triple: &str) -> Result<FileLock> {
        let fs = self.path(triple);

        fs.open_ro(".sentinel", &format!("{}'s sysroot", triple))
            .chain_err(|| {
                format!("couldn't lock {}'s sysroot as read-only", triple)
            })
    }

    pub fn lock_rw(&self, triple: &str) -> Result<FileLock> {
        let fs = self.path(triple);

        fs.open_rw(".sentinel", &format!("{}'s sysroot", triple))
            .chain_err(|| {
                format!("couldn't lock {}'s sysroot as read-only", triple)
            })
    }
}

pub fn home(cmode: &CompilationMode) -> Result<Home> {
    let mut p = if let Some(h) = env::var_os("XARGO_HOME") {
        PathBuf::from(h)
    } else {
        home::home_dir()
            .ok_or_else(|| "couldn't find your home directory. Is $HOME set?")?
            .join(".xargo")
    };

    if cmode.is_native() {
        p.push("HOST");
    }

    Ok(Home {
        path: Filesystem::new(p),
    })
}

pub struct Toml {
    table: Value,
}

impl Toml {
    /// Returns the `dependencies` part of `Xargo.toml`
    pub fn dependencies(&self) -> Option<&Value> {
        self.table.get("dependencies")
    }

    /// Returns the `target.{}.dependencies` part of `Xargo.toml`
    pub fn target_dependencies(&self, target: &str) -> Option<&Value> {
        self.table
            .get("target")
            .and_then(|t| t.get(target))
            .and_then(|t| t.get("dependencies"))
    }

    /// Returns the `patch` part of `Xargo.toml`
    pub fn patch(&self) -> Option<&Value> {
        self.table.get("patch")
    }
}

/// Returns the closest directory containing a 'Xargo.toml' and the parsed
/// content of this 'Xargo.toml'
pub fn toml(root: &Root) -> Result<(Option<&Path>, Option<Toml>)> {
    if let Some(p) = util::search(root.path(), "Xargo.toml") {
        Ok((Some(p), util::parse(&p.join("Xargo.toml")).map(|t| Some(Toml { table: t }))?))
    }
    else {
        Ok((None, None))
    }
}
