use std::{fs, io};
use std::path::Path;

use walkdir::WalkDir;

use errors::*;
use flock::FileLock;

pub fn cp_r(src: &Path, dst: &Path) -> Result<()> {
    fn cp_r_(src: &Path, dst: &Path) -> io::Result<()> {
        let nc = src.components().count();

        for entry in WalkDir::new(src) {
            let entry = entry?;
            let src = entry.path();

            let mut components = src.components();
            for _ in 0..nc {
                components.next();
            }
            let dst = dst.join(components.as_path());

            if entry.file_type().is_file() {
                fs::copy(src, dst)?;
            } else if !dst.exists() {
                fs::create_dir(dst)?;
            }
        }

        Ok(())
    }

    cp_r_(src, dst).chain_err(|| {
        format!("failed to recursively copy {} to {}",
                src.display(),
                dst.display())
    })
}

pub fn mkdir(path: &Path) -> Result<()> {
    fs::create_dir(path).chain_err(|| {
            format!("couldn't create a directory at {}", path.display())
        })?;
    Ok(())
}

pub fn remove_siblings(lock: &FileLock) -> Result<()> {
    lock.remove_siblings()
        .chain_err(|| {
            format!("couldn't clear the contents of {}",
                    lock.parent().display())
        })?;

    Ok(())
}
