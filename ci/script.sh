set -euxo pipefail

main() {
    cross build --target $TARGET
    cross run --target $TARGET -- -V

    if [ $TRAVIS_RUST_VERSION = nightly ]; then
        cargo test --features dev --target $TARGET
        cargo test --target $TARGET
    fi
}

if [ -z $TRAVIS_TAG ] && [ $TRAVIS_BRANCH != master ] || [ $TRAVIS_EVENT_TYPE = cron ]; then
    main
fi
