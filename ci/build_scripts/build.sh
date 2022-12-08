#!/bin/bash

set -euo pipefail

help() {
  cat <<EOF
Compile and build the tedge components and debian packages.
Cross is automatically used if you are trying to build for a foreign target (e.g. build for arm64 on a x86_64 machine)

By default, if the project is cloned using git (with all history), the version will be determined
via the tag and git history (e.g. using "git describe"). This enable the version to be automatically
incremented between official releases.

Alternatively, if you would like to set a custom version (for development/testing purposes), you can set
the 'GIT_SEMVER' environment variable before calling this script.

Usage:
    $0 [ARCH] [--use-cross]

Args:
    ARCH     RUST target architecture which can be a value listed from the command 'rustc --print target-list'
             If left blank then the TARGET will be set to that of the machine building the packages.

    Example ARCH (target) values:

        * x86_64-unknown-linux-gnu
        * aarch64-unknown-linux-gnu
        * aarch64-unknown-linux-musl
        * arm-unknown-linux-gnueabihf
        * armv7-unknown-linux-gnueabihf
        * armv7-unknown-linux-musleabihf

Flags:
    --use-cross     Force to use cross to build the packages
    --install-gcc   Install latest available gcc packages (for your operating system)

Env:
    GIT_SEMVER      Use a custom version when building the packages. Only use for dev/testing purposes!

Examples:
    $0
    # Build for the current CPU architecture

    $0 aarch64-unknown-linux-gnu
    # Build for arm64 linux (gnu lib)

    $0 x86_64-unknown-linux-gnu
    # Build for x86_64 linux (gnu lib)

    $0 armv7-unknown-linux-gnueabihf
    # Build for armv7 (armhf) linux (gnu lib)

    $0 armv7-unknown-linux-gnueabihf --install-gcc
    # Build for armv7 (armhf) linux (gnu lib) and install gcc automatically

    $0 --use-cross
    # Force to use cross when building for the current architecture

    export GIT_SEMVER=0.9.0-experiment-0.1
    $0 --use-cross
EOF
}

ARCH=
INSTALL_GCC=0
TARGET=()

REST_ARGS=()
while [ $# -gt 0 ]
do
    case "$1" in
        -c|--use-cross)
            USE_CROSS=1
            ;;

        --install-gcc)
            INSTALL_GCC=1
            ;;

        -h|--help)
            help
            exit 0
            ;;
        
        *)
            REST_ARGS+=("$1")
            ;;
    esac
    shift
done
set -- "${REST_ARGS[@]}"

if [ $# -eq 1 ]; then
    ARCH="$1"
fi

# Install required cargo crates
# cargo-deb >=1.41.3, the debian package names are automatically converted to a debian-conform name
cargo install cargo-deb --version 1.41.3

# Detect current host, and use cross to build if the target does not match the current host arch (triple)
HOST_ARCH=$(rustc -vV | sed -n 's|host: ||p')
USE_CROSS=${USE_CROSS:-}

if [ -z "$USE_CROSS" ]; then
    if [[ -n "$ARCH" && "$HOST_ARCH" != "$ARCH" ]]; then
        USE_CROSS=1
    else
        USE_CROSS=0
    fi
fi

BUILD_CMD=cargo
if [ "$USE_CROSS" == "1" ]; then
    cargo install cross
    echo "Using cross to compile binaries"
    BUILD_CMD=cross
fi

if [ -n "$ARCH" ]; then
    TARGET+=("--target=$ARCH")
fi

# Optionally install libc/libgcc dependencies for cross compiling and building the deb package
if [ "$INSTALL_GCC" == "1" ]; then
    case "$ARCH" in
        # e.g. armv5
        arm-unknown-linux-gnueabi)
            sudo apt-get -y update
            sudo apt-get -y install gcc-arm-linux-gnueabi
            ;;

        # e.g. armv6 (Raspberry Pi Zero)
        arm-unknown-linux-gnueabihf)
            sudo apt-get -y update
            sudo apt-get -y install -qq gcc-arm-linux-gnueabihf libc6-armhf-cross libc6-dev-armhf-cross
            ;;

        armv7-unknown-linux-gnueabihf)
            sudo apt-get -y update
            sudo apt-get -y install gcc-arm-linux-gnueabihf
            ;;

        aarch64-unknown-linux-gnu)
            sudo apt-get -y update
            sudo apt-get -y install gcc-aarch64-linux-gnu
            ;;

        x86_64-unknown-linux-*)
            sudo apt-get -y update
            sudo apt-get -y install gcc-x86-64-linux-gnu
            ;;
    esac
fi


# Load the release package list as $RELEASE_PACKAGES and $TEST_PACKAGES
# shellcheck disable=SC1091
source ./ci/package_list.sh

export GIT_SEMVER="${GIT_SEMVER:-}"

# Set version from scm
if [ -z "$GIT_SEMVER" ]; then
    if command -v git >/dev/null 2>&1; then
        GIT_DESCRIBE=$(git describe --always --tags --abbrev=8 2>/dev/null || true)

        # only match if it looks like a semver version
        if [[ "$GIT_DESCRIBE" =~ ^[0-9]+\.[0-9]+\.[0-9]+.*$ ]]; then
            GIT_SEMVER="$GIT_DESCRIBE"
            echo "Using version set from git: $GIT_SEMVER"
        else
            echo "git version does not match. got=$GIT_DESCRIBE, expected=^[0-9]+\.[0-9]+\.[0-9]+.*$"
        fi
    else
        echo "git is not present on system. version will be handled by cargo directly"
    fi
else
    echo "Using version set by user: $GIT_SEMVER"
fi

# build release for target
# GIT_SEMVER should be referenced in the build.rs scripts
"$BUILD_CMD" build --release "${TARGET[@]}"

# set cargo deb options
DEB_OPTIONS=()
if [ -n "$GIT_SEMVER" ]; then
    DEB_OPTIONS+=(
        --deb-version "$GIT_SEMVER"
    )
fi

# Create debian packages for release artifacts
for PACKAGE in "${RELEASE_PACKAGES[@]}"
do
    cargo deb -p "$PACKAGE" --no-strip --no-build "${DEB_OPTIONS[@]}" "${TARGET[@]}"
done

# Strip and build for test artifacts
for PACKAGE in "${TEST_PACKAGES[@]}"
do
    "$BUILD_CMD" build --release -p "$PACKAGE" "${TARGET[@]}"
    cargo deb -p "$PACKAGE" --no-strip --no-build "${DEB_OPTIONS[@]}" "${TARGET[@]}"
done
