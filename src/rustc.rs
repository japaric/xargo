extern crate rustc_version;

use std::ascii::AsciiExt;
use std::env;
use std::ffi::OsStr;
use std::path::{Component, PathBuf};
use std::process::Command;

use walkdir::WalkDir;

pub use self::rustc_version::{Channel, VersionMeta};

use cargo;
use errors::*;
use {CommandExt, Target};

/// The `rust-src` component within `rustc`'s sysroot
pub fn rust_src() -> Result<PathBuf> {
    let src = try!(sysroot()).join("lib/rustlib/src/rust");

    if src.join("src/libstd/Cargo.toml").is_file() {
        return Ok(src.to_owned());
    }

    for entry in WalkDir::new(try!(sysroot()).join("lib/rustlib/src")) {
        let entry =
            try!(entry.chain_err(|| "error recursively walking the sysroot"));

        if entry.file_type().is_file() && entry.file_name() == "Cargo.toml" {
            let path = entry.path();

            if let Some(parent) = path.parent() {
                if parent.components().rev().next() ==
                   Some(Component::Normal(OsStr::new("libstd"))) {
                    return Ok(parent.to_owned());
                }
            }
        }
    }

    try!(Err("`rust-src` component not found. Run `rustup component add \
              rust-src`."))
}

pub fn target_list() -> Result<Vec<String>> {
    let stdout =
        try!(rustc().args(&["--print", "target-list"]).run_and_get_stdout());

    Ok(stdout.split('\n')
        .filter_map(|s| if s.is_empty() {
            None
        } else {
            Some(s.to_owned())
        })
        .collect())
}

/// Parsed `rustc -Vv` output
pub fn meta() -> Result<VersionMeta> {
    Ok(rustc_version::version_meta_for(&try!(rustc()
        .arg("-Vv")
        .run_and_get_stdout())))
}

pub fn flags(target: &Target, tool: &str) -> Result<Vec<String>> {
    let tool = tool.to_ascii_uppercase();
    if let Ok(flags) = env::var(&tool) {
        return Ok(flags.split_whitespace().map(|s| s.to_owned()).collect());
    }

    let tool = tool.to_ascii_lowercase();
    if let Some(value) = try!(cargo::config()).and_then(|t| {
        t.lookup(&format!("target.{}.{}", target.triple(), tool))
            .or_else(|| t.lookup(&format!("build.{}", tool)))
            .cloned()
    }) {
        let mut error = false;
        let mut flags = vec![];
        if let Some(values) = value.as_slice() {
            for value in values {
                if let Some(flag) = value.as_str() {
                    flags.push(flag.to_owned());
                } else {
                    error = true;
                    break;
                }
            }
        } else {
            error = true;
        }

        if error {
            try!(Err(format!("{} in .cargo/config should be an array of \
                              string",
                             tool)))
        } else {
            Ok(flags)
        }
    } else {
        Ok(vec![])
    }

}

pub fn sysroot() -> Result<PathBuf> {
    Ok(PathBuf::from(try!(rustc()
            .arg("--print")
            .arg("sysroot")
            .run_and_get_stdout())
        .trim_right()))
}

pub fn rustc() -> Command {
    Command::new(env::var("RUSTC").as_ref().map(|s| &s[..]).unwrap_or("rustc"))
}
