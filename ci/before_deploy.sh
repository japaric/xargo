set -ex

mktempd() {
  echo $(mktemp -d 2>/dev/null || mktemp -d -t tmp)
}

mk_artifacts() {
  cargo build --target $TARGET --release
}

mk_tarball() {
  local temp_dir=$(mktempd)
  local out_dir=$(pwd)

  cp target/$TARGET/release/xargo $temp_dir

  pushd $temp_dir

  tar czf $out_dir/${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}.tar.gz *

  popd $temp_dir
  rm -r $temp_dir
}

main() {
  mk_artifacts
  mk_tarball
}

main
