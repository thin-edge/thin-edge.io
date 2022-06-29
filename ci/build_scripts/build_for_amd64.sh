#!/bin/bash -x

set -euo pipefail

# Install required cargo crates
cargo install cargo-deb --version 1.38.1

# Load the package list as $RELEASE_PACKAGES and $TEST_PACKAGES
source ./ci/package_list.sh

# Build release debian packages
for PACKAGE in "${RELEASE_PACKAGES[@]}"
do
    cargo deb -p "$PACKAGE"
done

# Build binaries required by test
for PACKAGE in "${TEST_PACKAGES[@]}"
do
    cargo build --release -p "$PACKAGE"
done
