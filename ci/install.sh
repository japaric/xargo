set -ex

case "$TRAVIS_OS_NAME" in
  linux)
    host=x86_64-unknown-linux-gnu
    ;;
  osx)
    host=x86_64-apple-darwin
    ;;
esac

mktempd() {
  echo $(mktemp -d 2>/dev/null || mktemp -d -t tmp)
}

install_rustup() {
  curl https://raw.githubusercontent.com/japaric/multirust-rs/pass-args/rustup-setup.sh -sSf | sh -s -- -y

  rustup default $CHANNEL
  rustc -V
  cargo -V
}

install_standard_crates() {
  if [ "$host" != "$TARGET" ]; then
    rustup target add $TARGET
  fi
}

main() {
  install_rustup
  install_standard_crates
}

main
