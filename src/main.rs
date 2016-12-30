#![deny(warnings)]

#[macro_use]
extern crate error_chain;
extern crate fs2;
extern crate libc;
extern crate rustc_version;
extern crate serde_json;
extern crate tempdir;
extern crate toml;
extern crate walkdir;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::{env, io, process};

use rustc_version::Channel;

use errors::*;
use rustc::Target;

mod cargo;
mod cli;
mod errors;
mod extensions;
mod flock;
mod rustc;
mod sysroot;
mod util;
mod xargo;

pub fn main() {
    fn show_backtrace() -> bool {
        env::var("RUST_BACKTRACE").as_ref().map(|s| &s[..]) == Ok("1")
    }

    match run() {
        Err(e) => {
            let stderr = io::stderr();
            let mut stderr = stderr.lock();

            writeln!(stderr, "error: {}", e).ok();

            for e in e.iter().skip(1) {
                writeln!(stderr, "caused by: {}", e).ok();
            }

            if show_backtrace() {
                if let Some(backtrace) = e.backtrace() {
                    writeln!(stderr, "{:?}", backtrace).ok();
                }
            } else {
                writeln!(stderr,
                         "note: run with `RUST_BACKTRACE=1` for a backtrace")
                    .ok();
            }

            process::exit(1)
        }
        Ok(status) => {
            if !status.success() {
                process::exit(status.code().unwrap_or(1))
            }
        }
    }
}

fn run() -> Result<ExitStatus> {
    let args = cli::args();
    let verbose = args.verbose();

    let meta = rustc::version();

    if let Some(sc) = args.subcommand() {
        if !sc.needs_sysroot() {
            return cargo::run(&args, verbose);
        }
    } else if args.version() {
        writeln!(io::stderr(),
                 concat!("xargo ", env!("CARGO_PKG_VERSION"), "{}"),
                 include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt")))
            .ok();

        return cargo::run(&args, verbose);
    }

    let cd = CurrentDirectory::get()?;

    let config = cargo::config()?;
    if let Some(root) = cargo::root()? {
        // We can't build sysroot with stable or beta due to unstable features
        let sysroot = rustc::sysroot(verbose)?;
        let src = match meta.channel {
            Channel::Dev => rustc::Src::from_env()?,
            Channel::Nightly => sysroot.src()?,
            Channel::Stable | Channel::Beta => return cargo::run(&args, verbose),
        };

        let target = if let Some(triple) = args.target() {
            if triple != meta.host {
                Target::new(triple, &cd, verbose)?
            } else {
                None
            }
        } else {
            if let Some(ref config) = config {
                if let Some(triple) = config.target()? {
                    if triple != meta.host {
                        Target::new(triple, &cd, verbose)?
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(target) = target {
            let home = xargo::home()?;
            let rustflags = cargo::rustflags(config.as_ref(), target.triple())?;

            sysroot::update(&target,
                            &home,
                            &root,
                            &rustflags,
                            &meta,
                            &src,
                            &sysroot,
                            verbose)?;
            return xargo::run(&args,
                              &target,
                              rustflags,
                              &home,
                              &meta,
                              config.as_ref(),
                              verbose);
        }
    }

    cargo::run(&args, verbose)
}

pub struct CurrentDirectory {
    path: PathBuf,
}

impl CurrentDirectory {
    fn get() -> Result<CurrentDirectory> {
        env::current_dir()
            .chain_err(|| "couldn't get the current directory")
            .map(|cd| CurrentDirectory { path: cd })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}
