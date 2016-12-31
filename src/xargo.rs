use std::path::{Display, PathBuf};
use std::process::{Command, ExitStatus};
use std::{env, mem};

use toml::Value;
use rustc_version::VersionMeta;

use cargo::{Config, Root, Rustflags, Subcommand};
use cli::Args;
use errors::*;
use extensions::CommandExt;
use flock::{FileLock, Filesystem};
use rustc::Target;
use {cargo, util};

pub fn run(args: &Args,
           target: &Target,
           rustflags: Rustflags,
           home: &Home,
           meta: &VersionMeta,
           config: Option<&Config>,
           verbose: bool)
           -> Result<ExitStatus> {
    let mut cmd = Command::new("cargo");
    cmd.args(args.all());

    if args.subcommand() == Some(Subcommand::Doc) {
        cmd.env("RUSTDOCFLAGS",
                cargo::rustdocflags(config, target.triple())?.for_xargo(home));
    }

    cmd.env("RUSTFLAGS", rustflags.for_xargo(home));

    let locks = (home.lock_ro(&meta.host), home.lock_ro(target.triple()));

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

    pub fn lock_ro(&self, target: &str) -> Result<FileLock> {
        self.path
            .join("lib/rustlib")
            .join(target)
            .open_ro(".sentinel", &format!("{}'s sysroot", target))
            .chain_err(|| {
                format!("couldn't lock {}'s sysroot as read-only", target)
            })
    }

    pub fn lock_rw(&self, target: &str) -> Result<FileLock> {
        self.path
            .join("lib/rustlib")
            .join(target)
            .open_rw(".sentinel", &format!("{}'s sysroot", target))
            .chain_err(|| {
                format!("couldn't lock {}'s sysroot as read-write", target)
            })
    }
}

pub fn home() -> Result<Home> {
    let p = if let Some(h) = env::var_os("XARGO_HOME") {
        PathBuf::from(h)
    } else {
        env::home_dir()
            .ok_or_else(|| "couldn't find your home directory. Is $HOME set?")?
            .join(".xargo")
    };

    Ok(Home { path: Filesystem::new(p) })
}

pub struct Toml {
    table: Value,
}

impl Toml {
    /// Returns the `target.{}.dependencies` part of `Xargo.toml`
    pub fn dependencies(&self, target: &str) -> Option<&Value> {
        self.table.lookup(&format!("target.{}.dependencies", target))
    }
}

pub fn toml(root: &Root) -> Result<Option<Toml>> {
    let p = root.path().join("Xargo.toml");

    if p.exists() {
        util::parse(&p).map(|t| Some(Toml { table: t }))
    } else {
        Ok(None)
    }
}
