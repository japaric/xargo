use std::env;

use errors::*;
use flock::{FileLock, Filesystem};

pub fn home() -> Result<Filesystem> {
    if let Some(home) = env::home_dir() {
        Ok(Filesystem::new(home.join(".xargo")))
    } else {
        Err("couldn't find your home directory. Is $HOME set?")?
    }
}

pub fn lock_ro(target: &str) -> Result<FileLock> {
    home()
        ?
        .join("lib/rustlib")
        .join(target)
        .open_ro(".sentinel", &format!("{}'s sysroot", target))
}

pub fn lock_rw(target: &str) -> Result<FileLock> {
    home()
        ?
        .join("lib/rustlib")
        .join(target)
        .open_rw(".sentinel", &format!("{}'s sysroot", target))
}
