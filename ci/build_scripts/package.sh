#!/usr/bin/env bash
set -e

# enable debugging  by default in ci
if [ "$CI" = "true" ]; then
    set -x
fi

help() {
  cat <<EOF
Build linux packages

NOTE: This script is intended to be called from the build.sh script

Usage:
    $0 <CMD> <ARCH> [...PACKAGE]

Args:
    CMD      Packaging command. Accepted values: [build, build_virtual]
             build   Build the linux packages
             build_virt  Build the virtual linux packages which make it easier for users to install, e.g. "tedge-full" just references all the tedge packages

    ARCH     RUST target architecture which can be a value listed from the command 'rustc --print target-list'
             If left blank then the TARGET will be set to the linux musl variant appropriate for your machine.
             For example, if building on MacOS M1, 'aarch64-unknown-linux-musl' will be selected, for linux x86_64,
             'x86_64-unknown-linux-musl' will be selected.
    
    PACKAGE  List of packages to build, e.g. tedge, tedge-agent, tedge-mapper etc. More than 1 can be provided

    Example ARCH (target) values:

        MUSL variants
        * x86_64-unknown-linux-musl
        * aarch64-unknown-linux-musl
        * armv7-unknown-linux-musleabihf
        * arm-unknown-linux-musleabihf

Flags:
    --help|-h   Show this help
    --version <string>      Use a specific version
    --output <path>         Output directory where the packages will be written to
    --types <csv_string>    CSV list of packages types. Accepted values: deb, rpm, apk, tarball
    --clean                 Clean the output directory before writing any packages to it

Env:
    GIT_SEMVER      Use a custom version when building the packages. Only use for dev/testing purposes!

Examples:
    $0 build aarch64-unknown-linux-musl tedge tedge-agent tedge-mapper
    # Package

    $0 aarch64-unknown-linux-musl tedge-agent
    # Package the tedge-agent for aarch64
EOF
}

#
# Package settings (what can be referenced in the nfpm configuration files)
#
export CI_PROJECT_URL="https://github.com/thin-edge/thin-edge.io"

#
# Script settings
#
OUTPUT_DIR=${OUTPUT_DIR:-dist}
TARGET=
VERSION=
CLEAN=1
PACKAGES=()
COMMAND=
PACKAGE_TYPES=

while [ $# -gt 0 ]
do
    case "$1" in
        --output)
            OUTPUT_DIR="$2"
            shift
            ;;
        --version)
            VERSION="$2"
            shift
            ;;
        --types)
            PACKAGE_TYPES="$2"
            shift
            ;;
        --clean)
            CLEAN=1
            ;;
        --no-clean)
            CLEAN=0
            ;;
        -h|--help)
            help
            exit 0
            ;;
        *)
            if [ -z "$COMMAND" ]; then
                COMMAND="$1"
            elif [ -z "$TARGET" ]; then
                TARGET="$1"
            else
                PACKAGES+=("$1")
            fi
            ;;
    esac
    shift
done

# Change to root project folder (to make referencing project files easier)
# and the script can be called from anywhere
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
pushd "$SCRIPT_DIR/../.." >/dev/null || exit 1

if [ -z "$TARGET" ]; then
    TARGET=$(./ci/build_scripts/detect_target.sh)
fi

if [ -z "$PACKAGE_TYPES" ]; then
    case "$TARGET" in
        *linux*|all) PACKAGE_TYPES="deb,apk,rpm,tarball" ;;
        *apple*) PACKAGE_TYPES="tarball" ;;
        *) PACKAGE_TYPES="tarball" ;;
    esac
fi

# Normalize output dir
OUTPUT_DIR="${OUTPUT_DIR%/}"

build_package() {
    name="$1"
    target="$2"

    package_arch=$(get_package_arch "$target")
    source_dir="target/$target/release"

    export PKG_ARCH="$package_arch"
    export PKG_NAME="$name"

    # Use symlinks to allow a fixed base directory in the nfpm.yaml definition
    rm -f .build
    ln -s "$source_dir" .build
    echo
    echo "Building: name=$name pkg_arch=$PKG_ARCH, source=$source_dir"

    COMMON_ARGS=(
        package
        -f "configuration/package_manifests/nfpm.$name.yaml"
        --target "$OUTPUT_DIR"
    )

    #
    # Debian/Ubuntu
    #
    # Special case for arm v6 on debian, since there is a name clash
    # * arm6 => armhf
    # * arm7 => armhf
    if [[ "$PACKAGE_TYPES" =~ deb ]]; then
        if [ "$package_arch" == "arm6" ]; then
            env GIT_SEMVER="${DEB_VERSION:-$GIT_SEMVER}" nfpm "${COMMON_ARGS[@]}" --packager deb --target "${OUTPUT_DIR}/${name}_${GIT_SEMVER}_armv6.deb"
        else
            env GIT_SEMVER="${DEB_VERSION:-$GIT_SEMVER}" nfpm "${COMMON_ARGS[@]}" --packager deb
        fi
    fi

    # RPM for CentOS/RHEL/RockyLinux
    if [[ "$PACKAGE_TYPES" =~ rpm ]]; then
        env GIT_SEMVER="${RPM_VERSION:-$GIT_SEMVER}" nfpm "${COMMON_ARGS[@]}" --packager rpm
    fi

    # Alpine
    if [[ "$PACKAGE_TYPES" =~ apk ]]; then
        env GIT_SEMVER="${APK_VERSION:-$GIT_SEMVER}" RELEASE="r0" nfpm "${COMMON_ARGS[@]}" --packager apk
    fi
}

build_virtual_package() {
    name="$1"
    COMMON_ARGS=(
        package
        -f "configuration/package_manifests/virtual/nfpm.$name.yaml"
        --target "$OUTPUT_DIR"
    )

    if [[ "$PACKAGE_TYPES" =~ deb ]]; then
        env GIT_SEMVER="${DEB_VERSION:-$GIT_SEMVER}" nfpm "${COMMON_ARGS[@]}" --packager deb
    fi

    if [[ "$PACKAGE_TYPES" =~ rpm ]]; then
        env GIT_SEMVER="${RPM_VERSION:-$GIT_SEMVER}" nfpm "${COMMON_ARGS[@]}" --packager rpm
    fi

    if [[ "$PACKAGE_TYPES" =~ apk ]]; then
        env GIT_SEMVER="${APK_VERSION:-$GIT_SEMVER}" RELEASE="r0" nfpm "${COMMON_ARGS[@]}" --packager apk
    fi

    # Expect to build at least 1 virtual package to prevent silent errors
    # when no virtual packages were created
    VIRTUAL_PACKAGE_COUNT=$(find target/virtual-packages -type f | wc -l | xargs)
    if [ "$VIRTUAL_PACKAGE_COUNT" -lt 1 ]; then
        echo "Expected at least 1 virtual packages to have been created. got=$VIRTUAL_PACKAGE_COUNT" >&2
        exit 1
    fi
}

get_package_arch() {
    # Return package architecture as per the 
    # nfpm architectures defined here: https://nfpm.goreleaser.com/goarch-to-pkg/
    # which uses the GOARCH to abstract across the different
    # arch names used in different linux packages (e.g. deb != rpm != apk)
    case "$1" in
        x86_64-unknown-linux-*) pkg_arch=amd64 ;;
        i686-unknown-linux-*) pkg_arch=386 ;;
        aarch64-unknown-linux-*) pkg_arch=arm64 ;;
        armv7-unknown-linux-*eabihf) pkg_arch=arm7 ;;
        arm-unknown-linux-*eabihf) pkg_arch=arm6 ;;
        arm-unknown-linux-*eabi) pkg_arch=arm6 ;;
        armv5te-unknown-linux-*eabi) pkg_arch=arm5 ;;
        riscv64gc-unknown-linux-*) pkg_arch=riscv64 ;;
        mips64el-unknown-linux-*abi64) pkg_arch=mips64le ;;
        mipsel-unknown-linux-*) pkg_arch=mipsle ;;
        aarch64-apple-darwin) pkg_arch=arm64 ;;
        x86_64-apple-darwin) pkg_arch=amd64 ;;
        *)
            echo "Unknown package architecture. value=$1"
            exit 1
            ;;
    esac
    echo "$pkg_arch"
}

build_tarball() {
    local name="$1"
    local target="$2"
    source_dir="target/$target/release"

    rm -f "$source_dir/$name"*tar.gz

    # Use underscores as a delimiter between version and target/arch to make it easier to parse
    TAR_FILE="${OUTPUT_DIR}/${name}_${TARBALL_VERSION:-$GIT_SEMVER}_${target}.tar.gz"

    echo ""
    echo "Building: pkg_arch=$target, source=$source_dir"
    echo "using tarball packager..."

    # Support both gnu tar (default) and bsd tar (for MacOS)
    tar_cmd="tar"
    tar_type="gnutar"
    if command -v gtar >/dev/null 2>&1; then
        tar_cmd="gtar"
    elif grep -q "GNU tar" <(tar --version); then
        tar_type="gnutar"
    elif grep -q "bsdtar" <(tar --version); then
        tar_type="bsdtar"
    fi

    # Only include the tedge binary as it is a multi-call binary
    case "$tar_type" in
        bsdtar)
            # bsd tar requires different options to prevent adding extra "AppleDouble" files, e.g. `._` files, to the archive
            echo "Using bsdtar, but please consider using gnu-tar instead. Install via: brew install gnu-tar"
            COPYFILE_DISABLE=1 tar cfz "$TAR_FILE" --no-xattrs --no-mac-metadata -C "$source_dir" --files-from <(printf "%s\n" "tedge")
            ;;
        *)
            # Default to gnu tar (as this is generally the default)
            "$tar_cmd" cfz "$TAR_FILE" --no-xattrs --owner=0 --group=0 --mode='0755' -C "$source_dir" --files-from <(printf "%s\n" "tedge")
            ;;
    esac

    echo "created package: $TAR_FILE"
}

cmd_build() {
    for name in "${PACKAGES[@]}"; do
        build_package "$name" "$TARGET"
    done

    if [[ "$PACKAGE_TYPES" =~ tarball ]]; then
        build_tarball "tedge" "$TARGET" "${PACKAGES[@]}"
    fi
}

prepare() {
    if [ "$CLEAN" = "1" ]; then
        echo "Cleaning output directory: $OUTPUT_DIR"
        rm -rf "$OUTPUT_DIR"
    fi
    mkdir -p "$OUTPUT_DIR"
}

banner() {
    local purpose="$1"
    echo ""
    echo "-----------------------------------------------------"
    echo "thin-edge.io packager: $purpose"
    echo "-----------------------------------------------------"
    echo "Parameters"
    echo ""
    echo "  packages: ${PACKAGES[*]}"
    echo "  version: $VERSION"
    echo "  types: $PACKAGE_TYPES"
    echo "  output_dir: $OUTPUT_DIR"
    echo ""
}

check_prerequisites() {
    if command -V nfpm >/dev/null 2>&1; then
        return
    fi

    # Try installing nfpm automatically before giving up
    if command -V go >/dev/null 2>&1; then
        go install github.com/goreleaser/nfpm/v2/cmd/nfpm@latest
        PATH="$(go env GOPATH)/bin:$PATH"
        export PATH
        return
    fi

    if command -V brew >/dev/null 2>&1; then
        brew install goreleaser/tap/nfpm
        brew install nfpm
        return
    fi    

    echo "Missing dependency: nfpm"
    echo "  Please install nfpm and try again: https://nfpm.goreleaser.com/install/"
    exit 1
}

check_prerequisites

# shellcheck disable=SC1091
. ./ci/build_scripts/version.sh --version "$VERSION"

case "$COMMAND" in
    build)
        banner "build"
        if command -v python3 >/dev/null 2>&1; then
            echo "Generating package scripts (e.g. postinst, postrm, preinst, prerm)"
            python3 configuration/package_scripts/generate.py >/dev/null
        fi
        prepare
        cmd_build
        ;;
    build_virtual)
        # Note: build_virtual does not support tarballs
        banner "build_virtual"
        prepare
        build_virtual_package "tedge-full"
        build_virtual_package "tedge-minimal"
        ;;
    *)
        echo "Unknown command. Accepted commands are: [build, build_virtual]"
        help
        exit 1
        ;;
esac

popd >/dev/null || exit 1

echo
echo "Successfully created packages"
echo
