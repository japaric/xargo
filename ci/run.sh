set -ex

test_mode() {
    cargo build --target $TARGET

    if [ $RUST_VERSION = nightly ]; then
        cargo test --target $TARGET
    fi
}

deploy_mode() {
    cargo rustc --target $TARGET --release -- -C lto
}

run() {
    if [ -z $TRAVIS_TAG ]; then
        test_mode
    else
        deploy_mode
    fi
}

run
