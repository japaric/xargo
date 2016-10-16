use std::env;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

struct IgnoredError {}

impl<E> From<E> for IgnoredError
    where E: Error
{
    fn from(_: E) -> IgnoredError {
        IgnoredError {}
    }
}

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    File::create(out_dir.join("commit-info.txt"))
        .unwrap()
        .write_all(commit_info().as_bytes())
        .unwrap();
}

fn commit_info() -> String {
    match (commit_hash(), commit_date()) {
        (Ok(hash), Ok(date)) => format!(" ({} {})", hash.trim(), date.trim()),
        _ => String::new(),
    }
}

fn commit_hash() -> Result<String, IgnoredError> {
    Ok(try!(String::from_utf8(try!(Command::new("git")
            .args(&["rev-parse", "--short", "HEAD"])
            .output())
        .stdout)))
}

fn commit_date() -> Result<String, IgnoredError> {
    Ok(try!(String::from_utf8(try!(Command::new("git")
            .args(&["log", "-1", "--date=short", "--pretty=format:%cd"])
            .output())
        .stdout)))
}
