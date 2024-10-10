#!/bin/sh
detect_linux_target() {
    # If no target has been given, choose the target triple based on the
    # host's architecture, however use the musl builds by default!
    host_arch="$(uname -m || true)"
    target_arch=""
    case "$host_arch" in
        x86_64*|amd64*)
            target_arch=x86_64-unknown-linux-musl
            ;;

        aarch64|arm64)
            target_arch=aarch64-unknown-linux-musl
            ;;

        armv7*)
            target_arch=armv7-unknown-linux-musleabihf
            ;;

        armv6*)
            target_arch=arm-unknown-linux-musleabi
            ;;
    esac
    echo "$target_arch"
}

detect_linux_target
