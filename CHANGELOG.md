# Change Log

All notable changes to this project will be documented in this file.
This project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

## [v0.1.3] - 2016-04-24

### Added

- `xargo (..) --verbose` passes `--verbose` to the `cargo` call that builds the sysroot.
- the sysroot now gets rebuilt when RUSTFLAGS or build.rustflags is modified.

### Fixed

- Xargo now respects the build.rustflags value set in .cargo/config.
- A bug where the hash/date file didn't get properly truncated before updating it leading to Xargo
to *always* trigger a sysroot rebuild.

## [v0.1.2] - 2016-04-24 [YANKED]

### Added

- Xargo now uses file locking and can be executed concurrently.
- Xargo now print its current status to the console while building a sysroot.
- Xargo now reports errors to the console instead of panicking.

### Removed

- Logging via `RUST_LOG` has been removed now that Xargo prints its status to the console.

## v0.1.1 - 2016-04-10

- Initial release

[Unreleased]: https://github.com/japaric/xargo/compare/v0.1.3...HEAD
[v0.1.3]: https://github.com/japaric/xargo/compare/v0.1.2...v0.1.3
[v0.1.2]: https://github.com/japaric/xargo/compare/v0.1.1...v0.1.2
