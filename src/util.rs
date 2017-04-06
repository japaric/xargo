use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::{fs, io};

use toml::{Parser, Value};

use errors::*;

pub fn cp_r(src: &Path, dst: &Path) -> Result<()> {
    (|| -> io::Result<()> {
            for e in src.read_dir()? {
                let e = e?;
                let metadata = e.path().metadata()?;
                let f = e.file_name();
                let src = src.join(&f);
                let dst = dst.join(&f);

                if metadata.is_dir() {
                    fs::create_dir(&dst)?;
                    cp_r(
                        src.as_path(),
                        dst.as_path()
                    ).map_err(|e| io::Error::new(io::ErrorKind::Other, e.description()))?;
                } else {
                    fs::copy(src, dst)?;
                }
            }

            Ok(())
        })()
        .chain_err(|| {
            format!("copying files from `{}` to `{}` failed",
                    src.display(),
                    dst.display())
        })
}

pub fn mkdir(path: &Path) -> Result<()> {
    fs::create_dir(path)
        .chain_err(|| format!("couldn't create directory {}", path.display()))
}

/// Parses `path` as TOML
pub fn parse(path: &Path) -> Result<Value> {
    Ok(Value::Table(Parser::new(&read(path)?).parse()
        .ok_or_else(|| format!("{} is not valid TOML", path.display()))?))
}

pub fn read(path: &Path) -> Result<String> {
    let mut s = String::new();

    let p = path.display();
    File::open(path).chain_err(|| format!("couldn't open {}", p))?
        .read_to_string(&mut s)
        .chain_err(|| format!("couldn't read {}", p))?;

    Ok(s)
}

/// Search for `file` in `path` and its parent directories
pub fn search<'p>(mut path: &'p Path, file: &str) -> Option<&'p Path> {
    loop {
        if path.join(file).exists() {
            return Some(path);
        }

        if let Some(p) = path.parent() {
            path = p;
        } else {
            return None;
        }
    }
}

pub fn write(path: &Path, contents: &str) -> Result<()> {
    let p = path.display();
    File::create(path)
        .chain_err(|| format!("couldn't open {}", p))?
        .write_all(contents.as_bytes())
        .chain_err(|| format!("couldn't write to {}", p))
}
