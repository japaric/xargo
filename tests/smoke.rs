extern crate tempdir;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::process::Command;
use std::io::Write;

use tempdir::TempDir;

macro_rules! try {
    ($e:expr) => {
        $e.unwrap_or_else(|e| panic!("{} with {}", stringify!($e), e))
    }
}

const CRATES: &'static [&'static str] = &["alloc", "collections", "core", "rand", "rustc_unicode"];
const LIB_RS: &'static [u8] = b"#![no_std]";

const CUSTOM_JSON: &'static str = r#"
    {
      "arch": "arm",
      "llvm-target": "thumbv7m-none-eabi",
      "os": "none",
      "target-endian": "little",
      "target-pointer-width": "32"
    }
"#;

fn xargo() -> Command {
    let mut path = try!(env::current_exe());
    path.pop();
    path.push("xargo");
    Command::new(path)
}

fn run(cmd: &mut Command) {
    println!("running: {:?}", cmd);
    let output = try!(cmd.output());
    if !output.status.success() {
        println!("--- stdout:\n{}", String::from_utf8_lossy(&output.stdout));
        println!("--- stderr:\n{}", String::from_utf8_lossy(&output.stderr));
        panic!("expected success, got: {}", output.status);
    }
}

fn exists_rlib(krate: &str, target: &str) -> bool {
    let home = env::home_dir().unwrap();

    for entry in try!(fs::read_dir(home.join(format!(".xargo/lib/rustlib/{}/lib", target)))) {
        let path = &try!(entry).path();

        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rlib") &&
           path.file_stem()
               .and_then(|f| f.to_str())
               .map(|s| s.starts_with(&format!("lib{}", krate))) == Some(true) {
            return true;
        }
    }

    false
}

fn cleanup(target: &str) {
    try!(fs::remove_dir_all(env::home_dir()
                                .unwrap()
                                .join(format!(".xargo/lib/rustlib/{}", target))));
}

#[test]
fn simple() {
    const TARGET: &'static str = "__simple";

    let td = try!(TempDir::new("xargo"));
    let td = &td.path();
    try!(try!(File::create(td.join(format!("{}.json", TARGET)))).write_all(CUSTOM_JSON.as_bytes()));

    run(xargo().args(&["init", "--vcs", "none", "--name", TARGET]).current_dir(td));
    try!(try!(OpenOptions::new().truncate(true).write(true).open(td.join("src/lib.rs")))
             .write_all(LIB_RS));
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    cleanup(TARGET);
}


// Check that `xargo build` builds a sysroot for the default target in .cargo/config
#[test]
fn cargo_config() {
    const CONFIG: &'static str = "[build]\ntarget = '{}'";
    const TARGET: &'static str = "__cargo_config";

    let td = try!(TempDir::new("xargo"));
    let td = &td.path();
    try!(try!(File::create(td.join(format!("{}.json", TARGET)))).write_all(CUSTOM_JSON.as_bytes()));

    run(xargo().args(&["init", "--vcs", "none", "--name", TARGET]).current_dir(td));
    try!(try!(OpenOptions::new().truncate(true).write(true).open(td.join("src/lib.rs")))
             .write_all(LIB_RS));
    try!(fs::create_dir(td.join(".cargo")));
    try!(try!(File::create(td.join(".cargo/config")))
             .write_all(CONFIG.replace("{}", TARGET).as_bytes()));
    run(xargo().arg("build").current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    cleanup(TARGET);
}

// Check that `--targer foo` overrides the default target in .cargo/config
#[test]
fn override_cargo_config() {
    const CONFIG: &'static [u8] = b"[build]\ntarget = 'dummy'";
    const TARGET: &'static str = "__override_cargo_config";

    let td = try!(TempDir::new("xargo"));
    let td = &td.path();
    try!(try!(File::create(td.join(format!("{}.json", TARGET)))).write_all(CUSTOM_JSON.as_bytes()));

    run(xargo().args(&["init", "--vcs", "none", "--name", TARGET]).current_dir(td));
    try!(try!(OpenOptions::new().truncate(true).write(true).open(td.join("src/lib.rs")))
             .write_all(LIB_RS));
    try!(fs::create_dir(td.join(".cargo")));
    try!(try!(File::create(td.join(".cargo/config"))).write_all(CONFIG));
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    cleanup(TARGET);
}
