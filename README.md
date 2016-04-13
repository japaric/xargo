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

```
$ git clone https://github.com/japaric/cu
$ cd cu
$ RUST_LOG=xargo xargo build --target stm32f100 --verbose
INFO:xargo: target: stm32f100
INFO:xargo::sysroot: rustc is a nightly from: 2016-03-25
INFO:xargo::sysroot: fetching source tarball
INFO:xargo::sysroot: unpacking source tarball
INFO:xargo::sysroot: cross compiling crates
INFO:xargo::sysroot: calling: "cargo" "build" "--release" "--target" "stm32f100" "-p" "collections" "-p" "rand"
INFO:xargo::sysroot:
   Compiling core v0.0.0 (file:///home/japaric/.xargo/src/libcore)
   Compiling alloc v0.0.0 (file:///home/japaric/.xargo/src/liballoc)
   Compiling rustc_unicode v0.0.0 (file:///home/japaric/.xargo/src/librustc_unicode)
   Compiling rand v0.0.0 (file:///home/japaric/.xargo/src/librand)
   Compiling collections v0.0.0 (file:///home/japaric/.xargo/src/libcollections)

INFO:xargo::sysroot: symlinking host crates
   Compiling rlibc v1.0.0
     Running `rustc (...) --sysroot /home/japaric/.xargo`
   Compiling compiler-rt v0.1.0 (file:///home/japaric/tmp/cu/compiler-rt)
     Running `rustc (...) --sysroot /home/japaric/.xargo`
   Compiling cu v0.1.0 (file:///home/japaric/tmp/cu)
     Running `rustc src/lib.rs (...) --sysroot /home/japaric/.xargo`
 (...)
```

`xargo` will cache the sysroot, so you can use it across different Cargo projects without having to
build the sysroot for each project. `xargo` will also take care of rebuilding the sysroot when
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

## Installation

```
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
