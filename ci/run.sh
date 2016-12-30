set -ex

main() {
    cargo build --target $TARGET
    cargo run --target $TARGET -- -V

    if [ $TRAVIS_RUST_VERSION = nightly ]; then
        cargo test --features dev --target $TARGET
        cargo test --target $TARGET
    fi
}

main
