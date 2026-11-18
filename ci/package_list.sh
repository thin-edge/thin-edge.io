#!/bin/bash

# Note: Must be in the expected installation order
RELEASE_PACKAGES=(
    tedge
    tedge-mapper
    tedge-agent
    tedge-watchdog
    tedge-apt-plugin
    c8y-remote-access-plugin
    c8y-firmware-plugin
    tedge-p11-server
)
export RELEASE_PACKAGES

# List of binaries which should be built
BINARIES=(
    tedge
    tedge-p11-server
)
export BINARIES

# Targets which the binaries are released for
# Note: keep in sync with the build matrix of .github/workflows/build-workflow.yml
RELEASE_TARGETS=(
    aarch64-unknown-linux-musl
    armv7-unknown-linux-musleabihf
    arm-unknown-linux-musleabihf
    arm-unknown-linux-musleabi
    armv5te-unknown-linux-musleabi
    x86_64-unknown-linux-musl
    i686-unknown-linux-musl
    riscv64gc-unknown-linux-musl
    aarch64-apple-darwin
    x86_64-apple-darwin
)
export RELEASE_TARGETS
