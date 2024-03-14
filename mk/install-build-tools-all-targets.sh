#!/usr/bin/env bash
set -e

TARGETS=(
    x86_64-unknown-linux-musl
    i686-unknown-linux-musl
    aarch64-unknown-linux-musl
    armv7-unknown-linux-musleabihf
    arm-unknown-linux-musleabihf
    arm-unknown-linux-musleabi
    armv5te-unknown-linux-musleabi
    riscv64gc-unknown-linux-gnu
)

for TARGET in "${TARGETS[@]}"; do
    mk/install-build-tools.sh --target="$TARGET"
done
