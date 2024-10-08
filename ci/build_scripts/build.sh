#!/usr/bin/env bash
# Allow user to use a more modern (locally) installed bash
# version by adding it to their PATH variable.

# Note: Don't use the -u option (for undefined variables), as the handling
# of empty arrays across bash versions is very inconsistent (e.g. Bash v3),
# and we rely on this to add optional arguments in this script. Bash v3 is still
# the default for some reason on MacOS as of 13.2.1 (Ventura).
# References: https://stackoverflow.com/questions/7577052/bash-empty-array-expansion-with-set-u
set -eo pipefail

help() {
  cat <<EOF
Compile and build the tedge components and linux packages.
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
             If left blank then the TARGET will be set to the linux musl variant appropriate for your machine.
             For example, if building on MacOS M1, 'aarch64-unknown-linux-musl' will be selected, for linux x86_64,
             'x86_64-unknown-linux-musl' will be selected.

    Example ARCH (target) values:

        Linux MUSL variants
        * x86_64-unknown-linux-musl
        * aarch64-unknown-linux-musl
        * armv7-unknown-linux-musleabihf
        * arm-unknown-linux-musleabihf

        Linux GNU variants
        * x86_64-unknown-linux-gnu
        * aarch64-unknown-linux-gnu
        * armv7-unknown-linux-gnueabihf
        * arm-unknown-linux-gnueabihf

        Apple
        * aarch64-apple-darwin
        * x86_64-apple-darwin

Flags:
    --help|-h   Show this help
    --skip-build    Skip building the binaries and only package them (e.g. just create the linux packages)

Env:
    GIT_SEMVER      Use a custom version when building the packages. Only use for dev/testing purposes!

Examples:
    $0
    # Build for the linux/musl target appropriate for the current CPU architecture

    $0 aarch64-unknown-linux-musl
    # Build for arm64 linux (musl)

    $0 x86_64-unknown-linux-musl
    # Build for x86_64 linux (musl)

    $0 armv7-unknown-linux-musleabihf
    # Build for armv7 (armhf) linux (musl)

    $0 arm-unknown-linux-musleabihf
    # Build for armv6 (armhf) linux (musl)

    $0 aarch64-unknown-linux-gnu
    # Build for arm64 linux (gnu lib)

    export GIT_SEMVER=0.9.0-experiment-0.1
    $0
    # Build using an manual version
EOF
}

ARCH=
TARGET=()
BUILD_OPTIONS=()
BUILD=1
INCLUDE_DEPRECATED_PACKAGES=0

REST_ARGS=()
while [ $# -gt 0 ]
do
    case "$1" in
        --skip-build)
            BUILD=0
            ;;

        --include-deprecated-packages)
            INCLUDE_DEPRECATED_PACKAGES=1
            ;;
        --skip-deprecated-packages)
            INCLUDE_DEPRECATED_PACKAGES=0
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

# Only set if rest arguments are defined
if [ ${#REST_ARGS[@]} -gt 0 ]; then
    set -- "${REST_ARGS[@]}"
fi

if [ $# -eq 1 ]; then
    ARCH="$1"
fi

# Set version from scm
# Run before installing any dependencies so that it
# can be called from other tools without requiring cargo
# shellcheck disable=SC1091
. ./ci/build_scripts/version.sh

if [ -z "$ARCH" ]; then
    # If no target has been given, choose the target triple based on the
    # host's architecture, however use the musl builds by default!
    HOST_ARCH="$(uname -m || true)"
    case "$HOST_ARCH" in
        x86_64*|amd64*)
            ARCH=x86_64-unknown-linux-musl
            ;;

        aarch64|arm64)
            ARCH=aarch64-unknown-linux-musl
            ;;

        armv7*)
            ARCH=armv7-unknown-linux-musleabihf
            ;;

        armv6*)
            ARCH=arm-unknown-linux-musleabihf
            ;;
    esac
fi


# Load the release package list as $RELEASE_PACKAGES, $DEPRECATED_PACKAGES
# shellcheck disable=SC1091
source ./ci/package_list.sh

# build release for target
# GIT_SEMVER should be referenced in the build.rs scripts
if [ "$BUILD" = 1 ]; then
    # Install stable toolchain if missing
    if command -V rustup >/dev/null 2>&1; then
        rustup toolchain install stable --no-self-update
    fi
    # Use zig to build as it is provides better cross compiling support
    cargo +stable install cargo-zigbuild --version ">=0.17.3"

    # Allow users to install zig by other package managers
    if ! zig --help &>/dev/null; then
        if ! python3 -m ziglang --help &>/dev/null; then
            PIP_ROOT_USER_ACTION=ignore pip3 install ziglang --break-system-packages 2>/dev/null || PIP_ROOT_USER_ACTION=ignore pip3 install ziglang
        fi
    fi

    # Display zig version to help with debugging
    echo "zig version: $(zig version 2>/dev/null || python3 -m ziglang version 2>/dev/null ||:)"

    if [ -n "$ARCH" ]; then
        echo "Using target: $ARCH"
        TARGET+=("--target=$ARCH")
        rustup target add "$ARCH"
    else
        # Note: This will build the artifacts under target/release and not target/<triple>/release !
        HOST_TARGET=$(rustc --version --verbose | grep host: | cut -d' ' -f2)
        echo "Using host target: $HOST_TARGET"
    fi

    # Custom options for different targets
    case "$ARCH" in
        *)
            BUILD_OPTIONS+=(
                --release
            )
            ;;
    esac

    cargo zigbuild "${TARGET[@]}" "${BUILD_OPTIONS[@]}"
fi

# Create release packages
OUTPUT_DIR="target/$ARCH/packages"

# Remove deprecated debian folder to avoid confusion with newly built linux packages
if [ -d "target/$ARCH/debian" ]; then
    echo "Removing deprecated debian folder created by cargo-deb: target/$ARCH/debian" >&2
    rm -rf "target/$ARCH/debian"
fi

PACKAGES=( "${RELEASE_PACKAGES[@]}" )
if [ "$INCLUDE_DEPRECATED_PACKAGES" = "1" ]; then
    PACKAGES+=(
        "${DEPRECATED_PACKAGES[@]}"
    )
fi

./ci/build_scripts/package.sh build "$ARCH" "${PACKAGES[@]}" --output "$OUTPUT_DIR"
