[![Travis](https://travis-ci.org/japaric/xargo.svg?branch=master)](https://travis-ci.org/japaric/xargo)
[![Appveyor](https://ci.appveyor.com/api/projects/status/5pb5okyox3te9dst?svg=true)](https://ci.appveyor.com/project/japaric/xargo)
[![crates.io](https://img.shields.io/crates/v/xargo.svg)](https://crates.io/crates/xargo)
[![crates.io](https://img.shields.io/crates/d/xargo.svg)](https://crates.io/crates/xargo)

# `xargo`

> Effortless cross compilation of Rust programs to custom bare-metal targets like ARM Cortex-M

## The problem

To cross compile Rust programs one needs standard crates like `libstd` or `libcore` that have been
cross compiled for the target. There are no official binaries of these crates for custom targets,
the ones that need custom target specification files, so one needs to cross compile them manually.
Furthermore, one needs to place these cross compiled crates in a specific directory layout, a
sysroot, so they can be picked up by `rustc` when the `--sysroot` flag is passed. Finally, to use
the sysroot with Cargo one needs to set the `RUSTFLAGs` variable to pass the `--sysroot` flag to
each `rustc` invocation.

These are too many steps prone to subtle errors like compiling Rust source code that was checked out
at a different commit hash than the one in `rustc -V`, etc. `xargo` makes the process
straightforward by taking care of all these steps and requiring zero effort on your part!

## Overview

`xargo` is a drop-in replacement for `cargo` . You can use it just like you would use `cargo`: with
standard commands like `xargo clean`, or with custom commands like `xargo fmt`.

The magic happens when you call `xargo` with the `--target` flag. In that case, `xargo` will take
care of building a sysroot with cross compiled crates and calling `cargo build` with the appropriate
`RUSTFLAGS` variable. Example below:

![Screenshot](http://i.imgur.com/pUIfnwu.jpg)

`xargo` will cache the sysroot, so you can use it across different Cargo projects without having to
build a sysroot for each project. `xargo` will also take care of rebuilding the sysroot when
`rustc` is updated or when the target specification file is modified.

## Caveats

- `xargo` only works with a nightly `rustc`/`cargo`.
- `xargo` will only build a sysroot for custom targets. For built-in targets (the ones in `rustc
    --print target-list`) you should install the standard crates via [rustup].
- Only freestanding crates (the ones that don't depend on `libc`) are cross compiled for the target.
- `xargo` doesn't cross compile `compiler-rt`.
- `xargo` ignores custom targets when `--target path/to/specification.json` is used.

[rustup]: https://www.rustup.rs/

## Dependencies

- `cargo` and `rustc` must be in $PATH
- Xargo depends on [the cargo crate], which depends on [libssh2-sys], which requires `cmake` and the
  OpenSSL headers to build (\*).
  - On Fedora, run:
    - `sudo dnf install cmake openssl-devel`
  - On Ubuntu, run
    - `sudo apt-get install cmake libssl-dev`
  - On Debian Jessie, building is currently broken by [this bug]. Adding the proposed-updates
    repository and updating to the fixed version of CMake will resolve the issue.
- If using a binary release instead of building Xargo on your own, you'll need the development
  version of both openssh and libcurl. `cmake` is not needed in this case.
  - On Ubuntu, run
    - `sudo apt-get install libcurl4-openssl-dev libssh2-1-dev`
  
[the cargo crate]: https://crates.io/crates/cargo
[libssh2-sys]: https://crates.io/crates/libssh2-sys
[this bug]: https://bugs.debian.org/cgi-bin/bugreport.cgi?bug=826656

## Installation

### Using a binary release

We have binary releases for the [three major platforms] supported by Rust. To install these
binaries, simply extract the tarball/zipfile and place the binary contained therein somewhere in you
PATH. If using rustup, it's recommended to place the binary in `~/.cargo/bin`, which is where rustup
is also installed.

[three major platforms]: https://github.com/japaric/xargo/releases 

### Build it yourself

```
cargo install xargo
```

If building on OSX 10.10 or greater, compilation of libssh2-sys will 
[fail due to missing OpenSSL headers](https://github.com/alexcrichton/ssh2-rs).
Make sure OpenSSL is installed via HomeBrew, then run the following command:

```bash
OPENSSL_ROOT_DIR=$(brew --prefix openssl)/ \
cargo install xargo
```

In case the build fails with the following error …

```
ld: library not found for -lssl
```

… you will have to use this more involved command instead:

```bash
LDFLAGS="$LDFLAGS -L$(brew --prefix openssl)/lib" \
CPPFLAGS="$CPPFLAGS -I$(brew --prefix openssl)/include" \
PKG_CONFIG_PATH="$PKG_CONFIG_PATH:$(brew --prefix openssl)/lib/pkgconfig" \
OPENSSL_ROOT_DIR=$(brew --prefix openssl)/ \
cargo install xargo
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
