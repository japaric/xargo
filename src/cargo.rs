use std::path::{Path, PathBuf};
use std::{env, fs};

use errors::*;
use toml::Value;
use toml;

fn current_dir() -> Result<PathBuf> {
    Ok(try!(env::current_dir()
        .chain_err(|| "couldn't get the current directory")))
}

/// Returns the parsed `.cargo/config` if any
pub fn config() -> Result<Option<Value>> {
    let config = search(&try!(current_dir()), ".cargo/config");

    if let Some(config) = config {
        Ok(Some(try!(toml::parse(&config))))
    } else {
        Ok(None)
    }
}

/// Returns the parsed `Cargo.toml`
pub fn toml() -> Result<Value> {
    if let Some(manifest) = search(&try!(current_dir()), "Cargo.toml") {
        Ok(try!(toml::parse(&manifest)))
    } else {
        try!(Err("not inside a Cargo project"))
    }
}

fn search(mut dir: &Path, rel_path: &str) -> Option<PathBuf> {
    loop {
        let file = dir.join(rel_path);

        if fs::metadata(&file).is_ok() {
            return Some(file);
        }

        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }

    None
}
