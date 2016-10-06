# Change Log

All notable changes to this project will be documented in this file.
This project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

### Added

- Xargo now builds a sysroot for the new built-in `thumbv*-none-eabi*` targets which don't ship with
  a binary release of the standard crates.

## [v0.1.12] - 2016-10-04

### Added

- Xargo now supports per-target rustflags: `target.thumbv7em-none-eabihf.rustflags` in
  .cargo/config.

## [v0.1.11] - 2016-09-30

### Fixed

- `xargo clean` and other commands not associated to building stuff no longer trigger a sysroot
  rebuild.

## [v0.1.10] - 2016-09-28

### Fixed

- `xargo doc`, which wasn't working because we didn't pass --sysroot to rustdoc. Note that rustdoc
  gained support for '--sysroot' as of nightly-2016-06-28, so that version or newer is required to
  use `xargo doc`.

## [v0.1.9] - 2016-09-27

### Fixed

- "error: Invalid cross-device link (os error 18)" which occurred when `$CARGO_HOME` was mounted in
  a different device than "`$XARGO_HOME`" (~/.xargo). The solution was to stop using hard links to
  place the host libraries in the Xargo sysroot and instead just copy them. This is a regression
  in disk usage but this problem was coming up in common Docker usage patterns (-v A:B).

## [v0.1.8] - 2016-09-04

### Changed

- All the status messages are now printed to stderr instead of to stdout. Cargo did the same change
  (from stdout to stderr) a while ago. Let's follow suit.

### Fixed

- When compiling crate `foo` with Xargo, the profile section of `foo`'s Cargo.toml is also "taken
  into account" when compiling the sysroot. For example, if `foo` has set `panic = "abort"` for all
  its profiles, then the sysroot will also be compiled with `-C panic=abort`. Previously, this
  wasn't the case.

## [v0.1.7] - 2016-09-03

### Fixed

- The sysroot now gets rebuilt when rust-src changes.

## [v0.1.6] - 2016-08-29

### Added

- Xargo can now use the source code installed by rustup. When available, this is the preferred way
  to fetch the source code and saves network bandwidth by not having to fetch the source tarball.

## [v0.1.5] - 2016-08-11

### Fixed

- Xargo now works properly when called from a `rustup override`n directory.

## [v0.1.4] - 2016-08-06

### Added

- Support targets that don't support atomics (`"max-atomic-width": 0`). For these targets, Xargo
  only compiles the `core` and `rustc_unicode` crates as the other crates depend on atomics (e.g.
  `alloc::Arc`).

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

[Unreleased]: https://github.com/japaric/xargo/compare/v0.1.12...HEAD
[v0.1.12]: https://github.com/japaric/xargo/compare/v0.1.11...v0.1.12
[v0.1.11]: https://github.com/japaric/xargo/compare/v0.1.10...v0.1.11
[v0.1.10]: https://github.com/japaric/xargo/compare/v0.1.9...v0.1.10
[v0.1.9]: https://github.com/japaric/xargo/compare/v0.1.8...v0.1.9
[v0.1.8]: https://github.com/japaric/xargo/compare/v0.1.7...v0.1.8
[v0.1.7]: https://github.com/japaric/xargo/compare/v0.1.6...v0.1.7
[v0.1.6]: https://github.com/japaric/xargo/compare/v0.1.5...v0.1.6
[v0.1.5]: https://github.com/japaric/xargo/compare/v0.1.4...v0.1.5
[v0.1.4]: https://github.com/japaric/xargo/compare/v0.1.3...v0.1.4
[v0.1.3]: https://github.com/japaric/xargo/compare/v0.1.2...v0.1.3
[v0.1.2]: https://github.com/japaric/xargo/compare/v0.1.1...v0.1.2
