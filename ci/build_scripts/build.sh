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
    $0 [ARCH]

Args:
    ARCH     RUST target architecture which can be a value listed from the command 'rustc --print target-list'
             If left blank then the TARGET will be set to that of the machine building the packages.

    Example ARCH (target) values:

        MUSL variants
        * x86_64-unknown-linux-musl
        * aarch64-unknown-linux-musl
        * armv7-unknown-linux-musleabihf
        * arm-unknown-linux-musleabihf

        GNU variants
        * x86_64-unknown-linux-gnu
        * aarch64-unknown-linux-gnu
        * armv7-unknown-linux-gnueabihf
        * arm-unknown-linux-gnueabihf

Flags:
    --help|-h   Show this help
    --version   Print the automatic version which will be used (this does not build the project)

Env:
    GIT_SEMVER      Use a custom version when building the packages. Only use for dev/testing purposes!

Examples:
    $0
    # Build for the current CPU architecture

    $0 aarch64-unknown-linux-musl
    # Build for arm64 linux (musl)

    $0 aarch64-unknown-linux-gnu
    # Build for arm64 linux (gnu lib)

    $0 x86_64-unknown-linux-musl
    # Build for x86_64 linux (musl)

    $0 armv7-unknown-linux-musleabihf
    # Build for armv7 (armhf) linux (musl)

    $0 arm-unknown-linux-musleabihf
    # Build for armv6 (armhf) linux (musl)

    export GIT_SEMVER=0.9.0-experiment-0.1
    $0
    # Build using an manual version
EOF
}

ARCH=
SHOW_VERSION=0
TARGET=()
BUILD_OPTIONS=()

REST_ARGS=()
while [ $# -gt 0 ]
do
    case "$1" in
        --version)
            SHOW_VERSION=1
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

export GIT_SEMVER="${GIT_SEMVER:-}"

# Set version from scm
# Run before installing any dependencies so that it
# can be called from other tools without requiring cargo
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

# Only show version (for usage with other tooling)
if [ "$SHOW_VERSION" == "1" ]; then
    echo "$GIT_SEMVER"
    exit 0
fi

# Install required cargo crates
# cargo-deb >=1.41.3, the debian package names are automatically converted to a debian-conform name
if ! cargo deb --help &>/dev/null; then
    cargo install cargo-deb --version 1.41.3
fi

# Use zig to build as it is provides better cross compiling support
if ! cargo zigbuild --help &>/dev/null; then
    cargo install cargo-zigbuild
fi

if ! python3 -c 'import ziglang' &>/dev/null; then
    pip3 install ziglang
fi

if [ -n "$ARCH" ]; then
    TARGET+=("--target=$ARCH")
    rustup target add "$ARCH"
fi

# Custom options for different targets
case "$ARCH" in
    *)
        BUILD_OPTIONS+=(
            --release
        )
        ;;
esac

# Load the release package list as $RELEASE_PACKAGES and $TEST_PACKAGES
# shellcheck disable=SC1091
source ./ci/package_list.sh

# build release for target
# GIT_SEMVER should be referenced in the build.rs scripts
cargo zigbuild "${TARGET[@]}" "${BUILD_OPTIONS[@]}"

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
    cargo zigbuild --release -p "$PACKAGE" "${TARGET[@]}"
    cargo deb -p "$PACKAGE" --no-strip --no-build "${DEB_OPTIONS[@]}" "${TARGET[@]}"
done
