set -euxo pipefail

beginswith() { case $2 in "$1"*) true;; *) false;; esac; }

main() {
    # We test against Cargo versions that don't support 'default-run'
    # As a workaround, we remove 'default-run' from the Cargo.toml
    # on CI
    # Unfortunately, we can't use 'sed -i', as this doesn't work on OS X
    sed 's/default-run = "xargo"//g' Cargo.toml > Cargo.toml.new
    mv Cargo.toml.new Cargo.toml
    cross build --target $TARGET --locked
    cross run --bin xargo --target $TARGET -- -V

    if beginswith nightly $TRAVIS_RUST_VERSION; then
        cargo test --features dev --target $TARGET
        cargo test --target $TARGET
    fi
}

main
