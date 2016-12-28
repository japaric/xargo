[![crates.io](https://img.shields.io/crates/v/xargo.svg)](https://crates.io/crates/xargo)
[![crates.io](https://img.shields.io/crates/d/xargo.svg)](https://crates.io/crates/xargo)

# `xargo`

> The sysroot manager that lets you build and customize `std`

<p align="center">
<img
  alt="Cross compiling `std` for i686-unknown-linux-gnu"
  src="assets/xargo.png"
  title="Cross compiling `std` for i686-unknown-linux-gnu"
>
<br>
<em>Cross compiling `std` for i686-unknown-linux-gnu</em>
</p>

Xargo builds and manages "sysroots" (cf. `rustc --print sysroot`). Making it
easy to cross compile Rust crates for targets that *don't* have binary
releases of the standard crates, like the `thumbv*m-none-eabi*` targets. And
it also lets you build a customized `std` crate, e.g. compiled with `-C
panic=abort`, for your target.

## Dependencies

- The `rust-src` component, which you can install with `rustup component add
  rust-src`.

- Rust and Cargo.

## Installation

```
$ cargo install xargo
```

But we also have [binary releases] for the three major OSes.

[binary releases]: https://github.com/japaric/xargo/releases

## Usage

`xargo` has the exact same CLI as `cargo`.

```
# This Just Works
$ xargo build --target thumbv6m-none-eabi
   Compiling core v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libcore)
    Finished release [optimized] target(s) in 11.61 secs
   Compiling lib v0.1.0 (file://$PWD)
    Finished debug [unoptimized + debuginfo] target(s) in 0.5 secs
```

`xargo` will cache the sysroot, in this case the `core` crate, so the next
`build` command will be (very) fast.

```
$ xargo build --target thumbv6m-none-eabi
    Finished debug [unoptimized + debuginfo] target(s) in 0.0 secs
```

By default, `xargo` will only compile the `core` crate for the target. If you
need a bigger subset of the standard crates, specify the dependencies in a
`Xargo.toml` at the root of your Cargo project (right next to `Cargo.toml`).

```
$ cat Xargo.toml
[target.thumbv6m-none-eabi.dependencies]
collections = {}

$ xargo build --target thumbv6m-none-eabi
   Compiling core v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libcore)
   Compiling alloc v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/liballoc)
   Compiling std_unicode v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libstd_unicode)
   Compiling collections v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libcollections)
    Finished release [optimized] target(s) in 15.26 secs
   Compiling lib v0.1.0 (file://$PWD)
    Finished debug [unoptimized + debuginfo] target(s) in 0.5 secs
```

You can compile a customized `std` crate as well, just specify which Cargo
features to enable.

```
# Build `std` with `-C panic=abort` (default) and with jemalloc as the default
# allocator
$ cat Xargo.toml
[target.i686-unknown-linux-gnu.dependencies.std]
features = ["jemalloc"]

# Needed to compile `std` with `-C panic=abort`
$ tail -n2 Cargo.toml
[profile.release]
panic = "abort"

$ xargo run --target i686-unknown-linux-gnu --release
    Updating registry `https://github.com/rust-lang/crates.io-index`
   Compiling libc v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/rustc/libc_shim)
   Compiling core v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libcore)
   Compiling build_helper v0.1.0 (file://$SYSROOT/lib/rustlib/src/rust/src/build_helper)
   Compiling gcc v0.3.41
   Compiling unwind v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libunwind)
   Compiling std v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libstd)
   Compiling compiler_builtins v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libcompiler_builtins)
   Compiling alloc_jemalloc v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/liballoc_jemalloc)
   Compiling alloc v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/liballoc)
   Compiling rand v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/librand)
   Compiling std_unicode v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libstd_unicode)
   Compiling alloc_system v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/liballoc_system)
   Compiling panic_abort v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libpanic_abort)
   Compiling collections v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libcollections)
    Finished release [optimized] target(s) in 33.49 secs
   Compiling hello v0.1.0 (file://$PWD)
    Finished release [optimized] target(s) in 0.28 secs
     Running `target/i686-unknown-linux-gnu/release/hello`
Hello, world!
```

If you'd like to know what `xargo` is doing under the hood, pass the verbose,
`-v`, flag to it.

```
$ xargo build --target thumbv6m-none-eabi -v
+ "rustc" "--print" "target-list"
+ "rustc" "--print" "sysroot"
+ "cargo" "build" "--release" "--manifest-path" "/tmp/xargo.lTBXKnaUGicV/Cargo.toml" "--target" "thumbv6m-none-eabi" "-v" "-p" "core"
   Compiling core v0.0.0 (file://$SYSROOT/lib/rustlib/src/rust/src/libcore)
     Running `rustc --crate-name core $SYSROOT/lib/rustlib/src/rust/src/libcore/lib.rs --crate-type lib -C opt-level=3 -C metadata=a5c596f87f7d486b -C extra-filename=-a5c596f87f7d486b --out-dir /tmp/xargo.lTBXKnaUGicV/target/thumbv6m-none-eabi/release/deps --emit=dep-info,link --target thumbv6m-none-eabi -L dependency=/tmp/xargo.lTBXKnaUGicV/target/thumbv6m-none-eabi/release/deps -L dependency=/tmp/xargo.lTBXKnaUGicV/target/release/deps`
    Finished release [optimized] target(s) in 11.50 secs
+ "cargo" "build" "--target" "thumbv6m-none-eabi" "-v"
   Compiling lib v0.1.0 (file://$PWD)
     Running `rustc --crate-name lib src/lib.rs --crate-type lib -g -C metadata=461fd0b398821543 -C extra-filename=-461fd0b398821543 --out-dir $PWD/target/thumbv6m-none-eabi/debug/deps --emit=dep-info,link --target thumbv6m-none-eabi -L dependency=$PWD/target/thumbv6m-none-eabi/debug/deps -L dependency=$PWD/lib/target/debug/deps --sysroot $HOME/.xargo`
    Finished debug [unoptimized + debuginfo] target(s) in 0.5 secs
```

Oh, and if you want to use `xargo` to compile `std` using a "dev" `rustc`, you
can use the `XARGO_RUST_SRC` environment variable to tell `xargo` where the Rust
source is.

```
# The source of the `core` crate must be in `$XARGO_RUST_SRC/libcore`
$ export XARGO_RUST_SRC=/path/to/rust/src

$ xargo build --target msp430-none-elf
```

## Caveats / gotchas

- Xargo won't build a sysroot when used with stable or beta Rust. This is
  because `std` and other standard crates depend on unstable features so it's
  not possible to build the sysroot with stable or beta.

- Because of how sysroots work, `xargo` *can't*, and won't, build a sysroot for
  the HOST. IOW, `xargo` will only build/use sysroots when you are cross
  compiling.

- As of nightly-2016-12-19, `std` can't be compiled from the `rust-src`
  component *without* patching the source.

  - To build `std` *without* the "jemalloc" feature, apply the patch
    in [rust-lang/rust#37975](https://github.com/rust-lang/rust/pull/37975).

  - To build `std` *with* the "jemalloc" feature, you'll have to [fix the
    permissions](https://github.com/rust-lang/rust/issues/36488) of the
    `rust/src/jemalloc` directory. `chmod -R +x rust/src/jemalloc` should do the
    trick.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
