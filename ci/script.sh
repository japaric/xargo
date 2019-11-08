set -euxo pipefail

beginswith() { case $2 in "$1"*) true;; *) false;; esac; }

main() {
    cross build --target $TARGET --locked
    cross run --target $TARGET -- -V

    if beginswith nightly $TRAVIS_RUST_VERSION; then
        cargo test --features dev --target $TARGET
        cargo test --target $TARGET
    fi
}

main
