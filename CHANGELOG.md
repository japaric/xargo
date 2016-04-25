### Unreleased

#### Added

- `xargo (..) --verbose` passes `--verbose` to the `cargo` call that builds the sysroot.

#### Fixed

- Xargo now respects the build.rustflags value set in .cargo/config.

### v0.1.2 - 2016-04-24

#### Added

- Xargo now uses file locking and can be executed concurrently.
- Xargo now print its current status to the console while building a sysroot.
- Xargo now reports errors to the console instead of panicking.

#### Removed

- Logging via `RUST_LOG` has been removed now that Xargo prints its status to the console.

### v0.1.1 - 2016-04-10

- Initial release
