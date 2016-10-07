# `script` phase: you usually build, test and generate docs in this phase

set -ex

main() {
    if [ $TRAVIS_OS_NAME = osx ]; then
        export OPENSSL_ROOT_DIR=`brew --prefix openssl`;
    fi

    cargo build --target $TARGET --verbose

    if [ $TRAVIS_RUST_VERSION = nightly ]; then
        cargo test --target $TARGET
    fi
}

main
