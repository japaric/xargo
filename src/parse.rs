use std::env;
use std::process::Command;

use errors::*;
use {cargo, toml};

/// Arguments passed to Xargo
#[derive(Debug)]
pub struct Args {
    pub cmd: Command,
    pub subcommand: Option<String>,
    pub target: Option<String>,
    pub verbose: bool,
}

pub fn args() -> Result<Args> {
    let args: Vec<_> = env::args().skip(1).collect();

    let mut cmd = Command::new("cargo");
    for arg in &args {
        cmd.arg(arg);
    }

    let mut args = args.iter();
    let mut subcommand = None;
    let mut target = None;
    let mut verbose = false;
    while let Some(arg) = args.next() {
        if !arg.starts_with('-') {
            subcommand = subcommand.or_else(|| Some(arg.clone()));
        }

        if arg == "-v" || arg == "--verbose" {
            verbose = true;
        }

        if arg == "-V" || arg == "--version" {
            println!("xargo {}{}",
                     env!("CARGO_PKG_VERSION"),
                     include_str!(concat!(env!("OUT_DIR"),
                                          "/commit-info.txt")));
        }

        if arg.starts_with("--target=") {
            target =
                target.or_else(|| {
                    arg.split('=').skip(1).next().map(|s| s.to_owned())
                })
        } else if arg == "--target" {
            target = target.or_else(|| args.next().map(|s| s.to_owned()))
        } else if arg.contains("--target") && arg.contains(' ') {
            // Special case for `xargo watch 'build --target $triple'`
            let mut args = arg.split_whitespace();

            while let Some(arg) = args.next() {
                if arg.starts_with("--target=") {
                    target = target.or_else(|| {
                        arg.split('=').skip(1).next().map(|s| s.to_owned())
                    });
                } else if arg == "--target" {
                    target =
                        target.or_else(|| args.next().map(|s| s.to_owned()));
                }
            }
        }
    }

    Ok(Args {
        cmd: cmd,
        subcommand: subcommand,
        target: target,
        verbose: verbose,
    })
}

pub fn profile_in_cargo_toml() -> Result<Option<toml::Value>> {
    let toml = try!(cargo::toml());

    if let Some(profile) = toml.lookup("profile") {
        let mut table = toml::Table::new();
        table.insert("profile".to_owned(), profile.clone());
        Ok(Some(toml::Value::Table(table)))
    } else {
        Ok(None)
    }
}

pub fn target_in_cargo_config() -> Result<Option<String>> {
    if let Some(config) = try!(cargo::config()) {
        if let Some(target) = config.lookup("build.target") {
            if let Some(target) = target.as_str() {
                Ok(Some(target.to_owned()))
            } else {
                try!(Err("build.target in .cargo/config is not a string"))
            }
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}
