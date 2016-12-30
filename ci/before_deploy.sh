set -ex

main() {
    local src_dir=$(pwd)\
          stage=

    case $TRAVIS_OS_NAME in
        linux)
            stage=$(mktemp -d)
            ;;
        osx)
            stage=$(mktemp -d -t tmp)
            ;;
    esac

    cross rustc --target $TARGET --release -- -C lto
    cp target/$TARGET/release/$CRATE_NAME $stage/

    cd $stage
    tar czf $src_dir/$CRATE_NAME-$TRAVIS_TAG-$TARGET.tar.gz *
    cd $src_dir

    rm -rf $stage
}

main
