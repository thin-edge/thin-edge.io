#!/bin/bash -x

set -euo pipefail

ARCH=$1

# Install required cargo crates
cargo install cargo-deb --version 1.38.1
cargo install cross

# armv7 uses `arm-linux-gnueabihf-strip`; aarch64 uses `aarch64-linux-gnu-strip`
# It appears `aarch64-linux-gnu-strip` seems to work explicitly on other arm bins but not other way around.
sudo apt update
sudo apt-get --assume-yes install binutils-arm-linux-gnueabihf binutils-aarch64-linux-gnu

# Load the release package list as $RELEASE_PACKAGES and $TEST_PACKAGES
# shellcheck disable=SC1091
source ./ci/package_list.sh

# Cross build release for target
cross build --release --target="$ARCH"

# Strip and create debian packages for release artifacts
for PACKAGE in "${RELEASE_PACKAGES[@]}"
do
    arm-linux-gnueabihf-strip target/"$ARCH"/release/"$PACKAGE" || aarch64-linux-gnu-strip target/"$ARCH"/release/"$PACKAGE"
    cargo deb -p "$PACKAGE" --no-strip --no-build --target="$ARCH"
done

# Strip and build for test artifacts
for PACKAGE in "${TEST_PACKAGES[@]}"
do
    cross build --release -p "$PACKAGE" --target="$ARCH"
    arm-linux-gnueabihf-strip target/"$ARCH"/release/"$PACKAGE" || aarch64-linux-gnu-strip target/"$ARCH"/release/"$PACKAGE"
done
