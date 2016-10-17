set -ex

test_mode() {
    cargo build --target $TARGET
    cargo run --target $TARGET -- -V

    if [ $TRAVIS_RUST_VERSION = nightly ]; then
        cargo test --target $TARGET
    fi
}

deploy_mode() {
    cargo rustc --target $TARGET --release -- -C lto
}

run() {
    if [ -z $TRAVIS_TAG ]; then
        test_mode
    elif [ $TRAVIS_RUST_VERSION = $DEPLOY_VERSION ]; then
        deploy_mode
    fi
}

run
