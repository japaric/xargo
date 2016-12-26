extern crate tempdir;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::process::Command;
use std::io::Write;

use tempdir::TempDir;

macro_rules! t {
    ($e:expr) => {
        $e.unwrap_or_else(|e| panic!("{} with {}", stringify!($e), e))
    }
}

const CRATES: &'static [&'static str] =
    &["alloc", "collections", "core", "rand", "std_unicode"];
const LIB_RS: &'static [u8] = b"#![no_std]";

const CUSTOM_JSON: &'static str = r#"
    {
      "arch": "arm",
      "data-layout": "e-m:e-p:32:32-i64:64-v128:64:128-a:0:32-n32-S64",
      "llvm-target": "thumbv7m-none-eabi",
      "os": "none",
      "target-endian": "little",
      "target-pointer-width": "32"
    }
"#;

const NO_ATOMICS_JSON: &'static str = r#"
    {
      "arch": "arm",
      "data-layout": "e-m:e-p:32:32-i64:64-v128:64:128-a:0:32-n32-S64",
      "llvm-target": "thumbv6m-none-eabi",
      "max-atomic-width": 0,
      "os": "none",
      "target-endian": "little",
      "target-pointer-width": "32"
    }
"#;

fn xargo() -> Command {
    let mut path = t!(env::current_exe());
    path.pop();
    path.pop();
    path.push("xargo");
    Command::new(path)
}

fn run(cmd: &mut Command) {
    println!("running: {:?}", cmd);
    let output = t!(cmd.output());
    if !output.status.success() {
        println!("--- stdout:\n{}", String::from_utf8_lossy(&output.stdout));
        println!("--- stderr:\n{}", String::from_utf8_lossy(&output.stderr));
        panic!("expected success, got: {}", output.status);
    }
}

fn exists_rlib(krate: &str, target: &str) -> bool {
    let home = env::home_dir().unwrap();

    let libdir = home.join(format!(".xargo/lib/rustlib/{}/lib", target));
    for entry in t!(fs::read_dir(libdir)) {
        let path = &t!(entry).path();

        if path.is_file() &&
           path.extension().and_then(|e| e.to_str()) == Some("rlib") &&
           path.file_stem()
            .and_then(|f| f.to_str())
            .map(|s| s.starts_with(&format!("lib{}", krate))) ==
           Some(true) {
            return true;
        }
    }

    false
}

fn cleanup(target: &str) {
    let path = env::home_dir()
        .unwrap()
        .join(format!(".xargo/lib/rustlib/{}", target));

    if path.exists() {
        t!(fs::remove_dir_all(path));
    }
}

#[test]
fn simple() {
    const TARGET: &'static str = "__simple";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    cleanup(TARGET);
}

#[test]
fn doc() {
    const TARGET: &'static str = "__doc";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    run(xargo().args(&["doc", "--target", TARGET]).current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    cleanup(TARGET);
}

// Calling `xargo build` twice shouldn't trigger a sysroot rebuild
// The only case the sysroot would have to be rebuild is when the source is
// updated but that shouldn't happen when running this test suite.
#[test]
fn twice() {
    const TARGET: &'static str = "__twice";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    let output = t!(xargo()
        .args(&["build", "--target", TARGET])
        .current_dir(td)
        .output());

    assert!(output.status.success());

    assert!(t!(String::from_utf8(output.stderr))
        .lines()
        .all(|l| !l.contains("Compiling")));

    cleanup(TARGET);
}

// Check that `xargo build` builds a sysroot for the default target in
// .cargo/config
#[test]
fn cargo_config() {
    const CONFIG: &'static str = "[build]\ntarget = '{}'";
    const TARGET: &'static str = "__cargo_config";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    t!(fs::create_dir(td.join(".cargo")));
    t!(t!(File::create(td.join(".cargo/config")))
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

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    t!(fs::create_dir(td.join(".cargo")));
    t!(t!(File::create(td.join(".cargo/config"))).write_all(CONFIG));
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    cleanup(TARGET);
}

// Check that the rustflags in .cargo/config are used to build the sysroot
#[test]
fn rustflags_in_cargo_config() {
    const TARGET: &'static str = "__rustflags_in_cargo_config";
    const CARGO_CONFIG: &'static str = "[build]\nrustflags = ['--cfg', \
                                        'xargo']";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));
    t!(fs::create_dir(td.join(".cargo")));
    t!(t!(File::create(td.join(".cargo/config")))
        .write_all(CARGO_CONFIG.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    let output = t!(xargo()
        .args(&["build", "--target", TARGET, "--verbose"])
        .current_dir(td)
        .output());

    assert!(output.status.success());

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    let mut at_least_once = false;
    for line in t!(String::from_utf8(output.stderr)).lines() {
        if line.contains("Running") && line.contains("rustc") &&
           line.contains(TARGET) {
            at_least_once = true;
            assert!(line.contains("--cfg xargo"));
        }
    }

    assert!(at_least_once);

    cleanup(TARGET);
}

// Check that the sysroot is rebuilt when RUSTFLAGS is modified
#[test]
fn rebuild_on_modified_rustflags() {
    const TARGET: &'static str = "__rebuild_on_modified_rustflags";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    run(xargo()
        .args(&["build", "--target", TARGET, "--verbose"])
        .current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    let output = t!(xargo()
        .args(&["build", "--target", TARGET])
        .current_dir(td)
        .env("RUSTFLAGS", "--cfg xargo")
        .output());

    assert!(output.status.success());

    let stderr = t!(String::from_utf8(output.stderr));

    for krate in CRATES {
        assert!(stderr.lines()
            .any(|l| l.contains("Compiling") && l.contains(krate)));
        assert!(exists_rlib(krate, TARGET));
    }

    // Another call with the same RUSTFLAGS shouldn't trigger a rebuild
    let output = t!(xargo()
        .args(&["build", "--target", TARGET])
        .current_dir(td)
        .env("RUSTFLAGS", "--cfg xargo")
        .output());

    assert!(output.status.success());

    let stderr = t!(String::from_utf8(output.stderr));

    assert!(stderr.lines().all(|l| !l.contains("Compiling")));

    cleanup(TARGET);
}

// For targets that don't support atomics, Xargo only compiles the `core` crate
#[test]
fn no_atomics() {
    const TARGET: &'static str = "__no_atomics";
    const CRATES: &'static [&'static str] = &["core", "std_unicode"];

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(NO_ATOMICS_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    cleanup(TARGET);
}

#[test]
fn panic_abort() {
    const TARGET: &'static str = "__panic_abort";
    const PROFILES: &'static [u8] = b"
[profile.dev]
panic = \"abort\"

[profile.release]
panic = \"abort\"
";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    t!(t!(OpenOptions::new()
            .append(true)
            .write(true)
            .open(td.join("Cargo.toml")))
        .write_all(PROFILES));

    let output = t!(xargo()
        .args(&["build", "--target", TARGET, "--verbose"])
        .current_dir(td)
        .output());

    assert!(output.status.success());

    let stderr = t!(String::from_utf8(output.stderr));

    let mut at_least_once = false;
    for line in stderr.lines() {
        if line.contains("Running") && line.contains("rustc") &&
           line.contains(TARGET) {
            at_least_once = true;

            if !line.contains("-C panic=abort") {
                panic!("{}", line);
            }
            // assert!(line.contains("-C panic=abort"));
        }
    }

    assert!(at_least_once);

    cleanup(TARGET);
}

// Make sure we build a sysroot for the built-in `thumbv*` targets which don't
// ship with binary releases of the standard crates
#[test]
fn thumb() {
    const TARGET: &'static str = "thumbv7m-none-eabi";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));

    cleanup(TARGET);
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    for krate in CRATES {
        assert!(exists_rlib(krate, TARGET));
    }

    cleanup(TARGET);
}

// We should not rebuild the sysroot if profile.*.lto changed
#[test]
fn profile_lto_changed() {
    const TARGET: &'static str = "__profile_lto_changed";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    OpenOptions::new()
        .write(true)
        .append(true)
        .open(td.join("Cargo.toml"))
        .unwrap()
        .write_all(b"[profile.dev]\nlto = true")
        .unwrap();

    let output = t!(xargo()
        .args(&["build", "--target", TARGET])
        .current_dir(td)
        .output());

    assert!(output.status.success());

    assert!(t!(String::from_utf8(output.stderr))
        .lines()
        .all(|l| !(l.contains("Compiling") && l.contains("core"))));

    cleanup(TARGET);
}

// We should not rebuild the sysroot if the arguments we passed to the linker
// changed
#[test]
fn linker_flags_changed() {
    const TARGET: &'static str = "__linker_flags_changed";

    let td = t!(TempDir::new("xargo"));
    let td = &td.path();
    t!(t!(File::create(td.join(format!("{}.json", TARGET))))
        .write_all(CUSTOM_JSON.as_bytes()));

    run(xargo()
        .args(&["init", "--vcs", "none", "--name", TARGET])
        .current_dir(td));
    t!(t!(OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(td.join("src/lib.rs")))
        .write_all(LIB_RS));
    run(xargo().args(&["build", "--target", TARGET]).current_dir(td));

    fs::create_dir(td.join(".cargo")).unwrap();
    File::create(td.join(".cargo/config"))
        .unwrap()
        .write_all(format!("[target.{}]\nrustflags = [\"-C\", \
                            \"link-arg=-lfoo\"]",
                           TARGET)
            .as_bytes())
        .unwrap();

    let output = t!(xargo()
        .args(&["build", "--target", TARGET])
        .current_dir(td)
        .output());

    assert!(output.status.success());

    assert!(t!(String::from_utf8(output.stderr))
        .lines()
        .all(|l| !(l.contains("Compiling") && l.contains("core"))));

    cleanup(TARGET);
}
