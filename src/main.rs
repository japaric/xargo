#![deny(warnings)]

extern crate chrono;
extern crate curl;
extern crate env_logger;
extern crate flate2;
extern crate rustc_version;
extern crate tar;
extern crate tempdir;
extern crate toml;

#[macro_use]
extern crate log;

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::hash::{Hash, Hasher, SipHasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use toml::{Parser, Value};

// TODO proper error handling/reporting
macro_rules! try {
    ($e:expr) => {
        $e.unwrap_or_else(|e| panic!("{} with {}", stringify!($e), e))
    }
}

mod sysroot;

fn main() {
    init_logger();

    let (mut cargo, target) = parse_args();

    if let Some(target) = target.or_else(|| read_config()) {
        let sysroot = sysroot::create(&target);

        if let Some(mut rustflags) = env::var("RUSTFLAGS").ok() {
            rustflags.push_str(&format!(" --sysroot {}", sysroot.display()));
            cargo.env("RUSTFLAGS", rustflags);
        } else {
            cargo.env("RUSTFLAGS", format!("--sysroot {}", sysroot.display()));
        }
    }

    if let Some(code) = try!(try!(cargo.spawn()).wait()).code() {
        process::exit(code);
    }
}

fn init_logger() {
    try!(env_logger::init());
}

/// Custom target with specification file
#[derive(Debug)]
pub struct Target {
    hash: u64,
    path: PathBuf,
    triple: String,
}

impl Target {
    fn from(s: &str) -> Option<Target> {
        let path = &PathBuf::from(format!("{}.json", s));
        if path.is_file() {
            return Some(Target::from_path(path));
        }

        let target_path = &env::var_os("RUST_TARGET_PATH").unwrap_or(OsString::new());

        for dir in env::split_paths(target_path) {
            let path = &dir.join(path);

            if path.is_file() {
                return Some(Target::from_path(path));
            }
        }

        None
    }

    fn from_path(path: &Path) -> Target {
        fn hash(path: &Path) -> u64 {
            let h = &mut SipHasher::new();
            let contents = &mut String::new();
            try!(try!(File::open(path)).read_to_string(contents));
            contents.hash(h);
            h.finish()
        }

        let triple = path.file_stem().unwrap().to_string_lossy().into_owned();
        info!("target: {}", triple);
        Target {
            hash: hash(path),
            path: try!(fs::canonicalize(path)),
            triple: triple,
        }
    }
}

fn parse_args() -> (Command, Option<Target>) {
    let mut cmd = Command::new("cargo");
    let mut target = None;

    let mut next_is_target = false;
    for arg_os in env::args_os().skip(1) {
        for arg in arg_os.to_string_lossy().split(' ') {
            if target.is_none() {
                if next_is_target {
                    target = Target::from(arg);
                } else {
                    if arg == "--target" {
                        next_is_target = true;
                    } else if arg.starts_with("--target=") {
                        target = arg.split('=').skip(1).next().and_then(|s| Target::from(s))
                    }
                }
            }
        }

        cmd.arg(arg_os);
    }

    (cmd, target)
}

/// Retrieves the build.target field of .cargo/config
fn read_config() -> Option<Target> {
    let mut config = None;
    let mut current = &*try!(env::current_dir());

    // NOTE Same logic as cargo's src/cargo/util/config.rs
    loop {
        let possible = current.join(".cargo/config");

        if fs::metadata(&possible).is_ok() {
            config = Some(possible);
        }

        if let Some(p) = current.parent() {
            current = p;
        } else {
            break;
        }
    }

    config.or_else(|| {
              let home = &env::home_dir().unwrap();

              if !current.starts_with(home) {
                  let config = home.join(".cargo/config");

                  if fs::metadata(&config).is_ok() {
                      Some(config)
                  } else {
                      None
                  }
              } else {
                  None
              }
          })
          .and_then(|p| {
              let s = &mut String::new();
              try!(try!(File::open(p)).read_to_string(s));
              Value::Table(Parser::new(s).parse().unwrap())
                  .lookup("build.target")
                  .and_then(|v| Target::from(v.as_str().unwrap()))
          })
}
