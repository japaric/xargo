### v0.1.3 - 2016-04-24

#### Added

- `xargo (..) --verbose` passes `--verbose` to the `cargo` call that builds the sysroot.
- the sysroot now gets rebuilt when RUSTFLAGS or build.rustflags is modified.

#### Fixed

- Xargo now respects the build.rustflags value set in .cargo/config.
- A bug where the hash/date file didn't get properly truncated before updating it leading to Xargo
to *always* trigger a sysroot rebuild.

### v0.1.2 - 2016-04-24

**YANKED** due to a serious regression in the sysroot rebuild trigger mechanism.

#### Added

- Xargo now uses file locking and can be executed concurrently.
- Xargo now print its current status to the console while building a sysroot.
- Xargo now reports errors to the console instead of panicking.

#### Removed

- Logging via `RUST_LOG` has been removed now that Xargo prints its status to the console.

### v0.1.1 - 2016-04-10

- Initial release
