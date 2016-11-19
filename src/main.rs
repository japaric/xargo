#![allow(dead_code)]

extern crate daggy;
#[macro_use]
extern crate error_chain;
extern crate fs2;
extern crate libc;
extern crate serde_json;
extern crate tempdir;
extern crate walkdir;

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::{env, mem, process};

use parse::Args;
use errors::*;

mod cargo;
mod dag;
mod errors;
mod flock;
mod fs;
mod io;
mod parse;
mod rustc;
mod sysroot;
mod toml;
mod xargo;

fn main() {
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
    let Args { mut cmd, target, subcommand, verbose } = try!(parse::args());

    let target = if let Some(target) = target.as_ref() {
        Some(try!(Target::from(target)))
    } else if let Some(target) =
        try!(parse::target_in_cargo_config()) {
        Some(try!(Target::from(&target)))
    } else {
        None
    };

    let needs_sysroot = match subcommand.as_ref().map(|s| &s[..]) {
        // we don't need to rebuild the sysroot for these subcommands
        None | Some("clean") | Some("init") | Some("new") |
        Some("update") | Some("search") => None,
        _ => {
            if let Some(target) = target.as_ref() {
                if target.needs_sysroot() {
                    try!(sysroot::update(target, verbose));
                    Some(target)
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    let locks = if let Some(target) = needs_sysroot {
        // TODO(Filesystem.display()) this could be better ...
        let sysroot = format!("{}", try!(xargo::home()).display());

        if subcommand.as_ref().map(|s| &s[..]) == Some("doc") {
            let mut flags = try!(rustc::flags(target, "rustdocflags"));
            flags.push("--sysroot".to_owned());

            flags.push(sysroot.clone());

            cmd.env("RUSTDOCFLAGS", flags.join(" "));
        }

        let mut flags = try!(rustc::flags(target, "rustflags"));
        flags.push("--sysroot".to_owned());

        // TODO(Filesystem.display()) this could be better ...
        flags.push(sysroot);

        cmd.env("RUSTFLAGS", flags.join(" "));

        // Make sure the sysroot is not blown up while the Cargo command is
        // running
        Some((xargo::lock_ro(&try!(rustc::meta()).host),
              xargo::lock_ro(target.triple())))
    } else {
        None
    };

    let status = try!(cmd.status()
        .chain_err(|| "failed to execute `cargo`. Is it not installed?"));

    mem::drop(locks);

    Ok(status)
}

#[derive(Debug)]
pub enum Target {
    BuiltIn { triple: String },
    Custom { triple: String, json: PathBuf },
    Path { triple: String, json: PathBuf },
}

impl Target {
    fn from(target: &str) -> Result<Target> {
        let target_list = try!(rustc::target_list());

        let target = target.to_owned();
        if target_list.iter().any(|t| t == &target) {
            Ok(Target::BuiltIn { triple: target })
        } else if target.ends_with(".json") {
            if let Some(triple) = Path::new(&target)
                .file_stem()
                .and_then(|f| f.to_str()) {
                Ok(Target::Path {
                    json: PathBuf::from(&target),
                    triple: triple.to_owned(),
                })
            } else {
                try!(Err(format!("error extracting triple from {}", target)))
            }
        } else {
            let json = Path::new(&target).with_extension("json");

            if json.exists() {
                Ok(Target::Custom {
                    json: json,
                    triple: target,
                })
            } else {
                if let Some(target_dir) = env::var_os("RUST_TARGET_PATH") {
                    let json = PathBuf::from(target_dir)
                        .join(&target)
                        .with_extension("json");

                    if json.exists() {
                        return Ok(Target::Custom {
                            json: json,
                            triple: target,
                        });
                    }
                }

                try!(Err(format!("no target specification file found \
                                  for {}",
                                 target)))
            }
        }
    }
}

impl Target {
    fn hash<H>(&self, hasher: &mut H) -> Result<()>
        where H: Hasher
    {
        match *self {
            Target::BuiltIn { .. } => {}
            Target::Custom { ref json, .. } |
            Target::Path { ref json, .. } => try!(io::read(json)).hash(hasher),
        }

        Ok(())
    }

    fn needs_sysroot(&self) -> bool {
        match *self {
            Target::BuiltIn { ref triple } => {
                match &triple[..] {
                    "arm-unknown-linux-musleabi" |
                    "arm-unknown-linux-musleabihf" |
                    "armv7-unknown-linux-musleabihf" |
                    "thumbv6m-none-eabi" |
                    "thumbv7m-none-eabi" |
                    "thumbv7em-none-eabi" |
                    "thumbv7em-none-eabihf" => true,
                    _ => false,
                }
            }
            _ => true,
        }
    }

    fn triple(&self) -> &str {
        match *self {
            Target::BuiltIn { ref triple } |
            Target::Custom { ref triple, .. } |
            Target::Path { ref triple, .. } => triple,
        }
    }
}

trait CommandExt {
    fn run_and_get_status(&mut self) -> Result<ExitStatus>;
    fn run_and_get_stdout(&mut self) -> Result<String>;
    fn run_or_error(&mut self) -> Result<()>;
}

impl CommandExt for Command {
    fn run_and_get_status(&mut self) -> Result<ExitStatus> {
        let cmd = &format!("`{:?}`", self);

        Ok(try!(self.status()
            .chain_err(|| format!("failed to execute {}", cmd))))
    }

    fn run_and_get_stdout(&mut self) -> Result<String> {
        let cmd = &format!("`{:?}`", self);

        let output = try!(self.output()
            .chain_err(|| format!("failed to execute {}", cmd)));

        if !output.status.success() {
            try!(Err(format!("{} failed with exit status: {:?}.\nstderr:\n{}",
                             cmd,
                             output.status.code(),
                             String::from_utf8_lossy(&output.stderr))))
        }

        Ok(try!(String::from_utf8(output.stdout)
            .chain_err(|| format!("{} output was not UTF-8 encoded", cmd))))
    }

    fn run_or_error(&mut self) -> Result<()> {
        let cmd = &format!("`{:?}`", self);

        let exit = try!(self.status()
            .chain_err(|| format!("failed to execute {}", cmd)));

        if !exit.success() {
            try!(Err(format!("{} failed with exit status {:?}",
                             cmd,
                             exit.code())))
        }

        Ok(())
    }
}
