# `script` phase: you usually build, test and generate docs in this phase

set -ex

main() {
  cargo build --target $TARGET --verbose

  if [ $TRAVIS_RUST_VERSION = nightly ]; then
      cargo test --target $TARGET
  fi
}

main
