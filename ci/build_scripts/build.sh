#!/bin/bash -x

set -euo pipefail

help() {
  cat <<EOF
Compile and build the tedge components and debian packages.
Cross is automatically used if you are trying to build for a foreign target (e.g. build for arm64 on a x86_64 machine)

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

Examples:
    $0
    # Build for the current CPU architecture

    $0 aarch64-unknown-linux-gnu
    # Build for arm64 linux (gnu lib)

    $0 x86_64-unknown-linux-gnu
    # Build for x86_64 linux (gnu lib)

    $0 armv7-unknown-linux-gnueabihf
    # Build for armv7 (armhf) linux (gnu lib)

    $0 --use-cross
    # Force to use cross when building for the current architecture
EOF
}

ARCH=
TARGET=()

REST_ARGS=()
while [ $# -gt 0 ]
do
    case "$1" in
        -c|--use-cross)
            USE_CROSS=1
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
cargo install cargo-deb --version 1.38.1

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

# Install libc/libgcc dependencies for cross compiling and building the deb package
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


# Load the release package list as $RELEASE_PACKAGES and $TEST_PACKAGES
# shellcheck disable=SC1091
source ./ci/package_list.sh

# build release for target
"$BUILD_CMD" build --release "${TARGET[@]}"

# Work out which strip command
# Figure out what strip tool to use if any
STRIP=
if [ -n "$ARCH" ]; then
    STRIP="strip"

    case "$ARCH" in
        arm-unknown-linux-*) STRIP="arm-linux-gnueabihf-strip" ;;
        armv7-unknown-linux-*) STRIP="arm-linux-gnueabihf-strip" ;;
        aarch64-unknown-linux-gnu) STRIP="aarch64-linux-gnu-strip" ;;
        aarch64-unknown-linux-musl) STRIP="aarch64-linux-gnu-strip" ;;
        x86_64-unknown-linux-*) STRIP="x86_64-linux-gnu-strip" ;;
        *-pc-windows-msvc) STRIP="" ;;
    esac;
fi

# Strip and create debian packages for release artifacts
for PACKAGE in "${RELEASE_PACKAGES[@]}"
do
    if [ -n "$STRIP" ]; then
        "$STRIP" target/"$ARCH"/release/"$PACKAGE"
    fi

    cargo deb -p "$PACKAGE" --no-strip --no-build "${TARGET[@]}"
done

# Strip and build for test artifacts
for PACKAGE in "${TEST_PACKAGES[@]}"
do
    "$BUILD_CMD" build --release -p "$PACKAGE" "${TARGET[@]}"

    if [ -n "$STRIP" ]; then
        "$STRIP" target/"$ARCH"/release/"$PACKAGE"
    fi
done
