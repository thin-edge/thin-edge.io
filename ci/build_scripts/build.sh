#!/usr/bin/env bash
# Allow user to use a more modern (locally) installed bash
# version by adding it to their PATH variable.

# Note: Don't use the -u option (for undefined variables), as the handling
# of empty arrays across bash versions is very inconsistent (e.g. Bash v3),
# and we rely on this to add optional arguments in this script. Bash v3 is still
# the default for some reason on MacOS as of 13.2.1 (Ventura).
# References: https://stackoverflow.com/questions/7577052/bash-empty-array-expansion-with-set-u
set -eo pipefail

# enable debugging  by default in ci
if [ "$CI" = "true" ]; then
    set -x
fi

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
    $0 [TARGET]

Args:
    TARGET   RUST target architecture which can be a value listed from the command 'rustc --print target-list'
             If left blank then the TARGET will be set to the linux musl variant appropriate for your machine.
             For example, if building on MacOS M1, 'aarch64-unknown-linux-musl' will be selected, for linux x86_64,
             'x86_64-unknown-linux-musl' will be selected.

    Example TARGET values:

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
    --build-with <auto|zig|clang|native>    Choose which tooling to use to build the binaries. If set to 'auto',
                                            then the build tool will be selected based on the binary
    --bin <name>    Override which binary to build. By default the binaries form the package_list.sh will be used
    --glibc-version <version>   GLIBC version to use when compiling for libc targets
    --skip-build    Skip building the binaries and only package them (e.g. just create the linux packages)

Env:
    GIT_SEMVER      Use a custom version when building the packages. Only use for dev/testing purposes!

Examples:
    $0
    # Build for the linux/musl target appropriate for the current CPU architecture

    $0 aarch64-unknown-linux-musl
    # Build for arm64 linux (musl)

    $0 aarch64-unknown-linux-musl --build-with clang
    # Build for arm64 linux (musl) using clang (for linux only)

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

# Load the release package list as $RELEASE_PACKAGES, BINARIES
# shellcheck disable=SC1091
source ./ci/package_list.sh

TARGET="${TARGET:-}"
BUILD_WITH="${BUILD_WITH:-zig}"
COMMON_BUILD_OPTIONS=(
    "--release"
)
TOOLCHAIN="${TOOLCHAIN:-+1.85}"
# Note: Minimum version that is supported with riscv64gc-unknown-linux-gnu is 2.27
GLIBC_VERSION="${GLIBC_VERSION:-2.17}"
RISCV_GLIBC_VERSION="${RISCV_GLIBC_VERSION:-2.27}"
OVERRIDE_BINARIES=()
ARTIFACT_DIR="${ARTIFACT_DIR:-}"

BUILD=1

REST_ARGS=()
while [ $# -gt 0 ]
do
    case "$1" in
        --build-with)
            BUILD_WITH="$2"
            shift
            ;;
        --bin)
            OVERRIDE_BINARIES=( "$2" )
            shift
            ;;
        --toolchain)
            TOOLCHAIN="+$2"
            shift
            ;;
        --glibc-version)
            GLIBC_VERSION="$2"
            shift
            ;;
        --skip-build)
            BUILD=0
            ;;
        --artifact-dir)
            ARTIFACT_DIR="$2"
            shift
            ;;
        # deprecated option. To be removed after usage of it is removed from the build-workflow
        --skip-deprecated-packages)
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

if [ ${#OVERRIDE_BINARIES[@]} -gt 0 ]; then
    # Override the list of binaries to build
    BINARIES=("${OVERRIDE_BINARIES[@]}")
fi

if [ $# -eq 1 ]; then
    TARGET="$1"
fi

if [ -z "$TARGET" ]; then
    TARGET=$(./ci/build_scripts/detect_target.sh)
fi

# Set version from scm
# Run before installing any dependencies so that it
# can be called from other tools without requiring cargo
# shellcheck disable=SC1091
source ./ci/build_scripts/version.sh

install_rust() {
    # Install toolchain if missing
    if command -V rustup >/dev/null 2>&1; then
        rustup toolchain install "${TOOLCHAIN//+/}" --no-self-update
    fi
}

install_zig_tools() {
    # zig provides better cross compiling support
    # shellcheck disable=SC2086
    cargo $TOOLCHAIN install cargo-zigbuild --version ">=0.17.3"

    # Allow users to install zig by other package managers
    if ! zig --help &>/dev/null; then
        if ! python3 -m ziglang --help &>/dev/null; then
            PIP_ROOT_USER_ACTION=ignore pip3 install ziglang --break-system-packages 2>/dev/null || PIP_ROOT_USER_ACTION=ignore pip3 install ziglang
        fi
    fi

    # Display zig version to help with debugging
    echo "zig version: $(zig version 2>/dev/null || python3 -m ziglang version 2>/dev/null ||:)"
}

build() {
    build_tool="$1"
    shift
    case "$build_tool" in
        zig|ziglang|cargo-zigbuild)
            # shellcheck disable=SC2086
            cargo-zigbuild $TOOLCHAIN zigbuild "$@"
            ;;
        clang)
            # shellcheck disable=SC2086
            mk/cargo.sh $TOOLCHAIN build "$@"
            ;;
        native|*)
            # shellcheck disable=SC2086
            cargo $TOOLCHAIN build "$@"
            ;;
    esac
}

get_build_tool_for_binary() {
    # Different binaries have different requirements / build dependencies
    # which influence which build tools can be used.
    # Previously tedge has been using clang to build the binaries, so clang
    # should still be preferred to reduce risk of unexpected differences between
    # different compiler optimizations (not sure if this is true, but less changes are generally safer)
    binary="$1"
    case "$binary" in
        tedge-p11-server)
            echo "zig"
            ;;
        *)
            echo "clang"
    esac
}

get_target_for_binary() {
    binary_name="$1"
    target="$2"
    case "$binary_name" in
        tedge-p11-server)
            # requires gnu target as loading .so files requires to be dynamically compiled
            # This can return the same output, but this is fine as apple targets support
            # loading by default
            echo "${target//musl/gnu}"
            ;;
        *)
            echo "$target"
            ;;
    esac
}

if [ -z "$ARTIFACT_DIR" ]; then
    ARTIFACT_DIR="target/$TARGET/release"
fi
mkdir -p "$ARTIFACT_DIR"

# build release for target
# GIT_SEMVER should be referenced in the build.rs scripts
if [ "$BUILD" = 1 ] && [ ${#BINARIES[@]} -gt 0 ]; then
    install_rust

    for name in "${BINARIES[@]}"; do
        BINARY_TARGET=$(get_target_for_binary "$name" "$TARGET")
        # shellcheck disable=SC2086
        rustup $TOOLCHAIN target add "$BINARY_TARGET"
        BUILD_DIR="target/$BINARY_TARGET/release"

        # Each binary should have its preferred build tool (unless if the user overrides this)
        BUILD_TOOL=$(get_build_tool_for_binary "$name")
        if [ -n "$BUILD_WITH" ] && [ "$BUILD_WITH" != "auto" ]; then
            BUILD_TOOL="$BUILD_WITH"
        fi

        case "$BUILD_TOOL" in
            zig)
                install_zig_tools

                case "$BINARY_TARGET" in
                    riscv64gc-unknown-linux-gnu)
                        # riscv is a newer processor so the minimum glibc version is higher than for other targets
                        if [ -n "$RISCV_GLIBC_VERSION" ]; then
                            BINARY_TARGET="${BINARY_TARGET}.${RISCV_GLIBC_VERSION}"
                        fi
                        ;;
                    *gnu*)
                        if [ -n "$GLIBC_VERSION" ]; then
                            BINARY_TARGET="${BINARY_TARGET}.${GLIBC_VERSION}"
                        fi
                        ;;
                esac
                ;;
            clang)
                # shellcheck disable=SC2086
                ./mk/install-build-tools.sh $TOOLCHAIN --target="$BINARY_TARGET"
                ;;
            *)
                ;;
        esac

        build "$BUILD_TOOL" --target="$BINARY_TARGET" "${COMMON_BUILD_OPTIONS[@]}" --bin "$name"
        if [ "$BUILD_DIR" != "$ARTIFACT_DIR" ]; then
            cp "$BUILD_DIR/$name" "$ARTIFACT_DIR/"
        fi
    done
fi

# Create release packages (e.g. linux packages like rpm, deb, apk etc.)
OUTPUT_DIR="$(dirname "$ARTIFACT_DIR")/packages"
PACKAGES=( "${RELEASE_PACKAGES[@]}" )
./ci/build_scripts/package.sh build "$TARGET" "${PACKAGES[@]}" --output "$OUTPUT_DIR"
