use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub use std::io::stderr;

use errors::*;
use flock::FileLock;

pub fn read(file: &Path) -> Result<String> {
    let mut string = String::new();
    let file_ = file.display();
    let mut f = try!(File::open(file)
        .chain_err(|| format!("couldn't open {}", file_)));
    f.read_to_string(&mut string)
        .chain_err(|| format!("couldn't read {}", file_))?;
    Ok(string)
}

pub fn read_hash(lock: &FileLock) -> Result<Option<u64>> {
    let path = lock.parent().join(".hash");

    if path.exists() {
        Ok(Some(read(&path)?
            .parse()
            .chain_err(|| format!("error parsing {}", path.display()))?))
    } else {
        Ok(None)
    }
}

pub fn write(file: &Path, contents: &str) -> Result<()> {
    let file_ = file.display();
    let mut f = try!(File::create(file)
        .chain_err(|| format!("couldn't create {}", file_)));
    f.write_all(contents.as_bytes())
        .chain_err(|| format!("couldn't write to {}", file_))?;
    Ok(())
}

pub fn write_hash(lock: &FileLock, hash: u64) -> Result<()> {
    write(&lock.parent().join(".hash"), &hash.to_string())
}
