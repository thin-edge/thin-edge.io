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
SHOW_VERSION=0
TARGET=()
BUILD_OPTIONS=()
BUILD=1

REST_ARGS=()
while [ $# -gt 0 ]
do
    case "$1" in
        --version)
            SHOW_VERSION=1
            ;;

        --skip-build)
            BUILD=0
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

export GIT_SEMVER="${GIT_SEMVER:-}"

next_base_version() {
    local version="$1"
    local bump_type="$2"
    local major
    local minor
    local patch
    major=$(echo "$version" | cut -d'.' -f1)
    minor=$(echo "$version" | cut -d'.' -f2)
    patch=$(echo "$version" | cut -d'.' -f3)
    
    case "$bump_type" in
        major)
            major=$((major + 1))
            ;;
        minor)
            minor=$((minor + 1))
            ;;
        patch)
            patch=$((patch + 1))
            ;;
        *)
            patch=$((patch + 1))
            ;;
    esac

    echo "${major}.${minor}.${patch}"
}

# Set version from scm
# Run before installing any dependencies so that it
# can be called from other tools without requiring cargo
if [ -z "$GIT_SEMVER" ]; then
    if command -v git >/dev/null 2>&1; then
        GIT_DESCRIBE_RAW=$(git describe --always --tags --abbrev=8 2>/dev/null || true)

        BASE_VERSION=$(echo "$GIT_DESCRIBE_RAW" | cut -d- -f1)
        BUILD_COMMITS_SINCE=$(echo "$GIT_DESCRIBE_RAW" | cut -d- -f2)
        BUILD_COMMIT_HASH=$(echo "$GIT_DESCRIBE_RAW" | cut -d- -f3)

        if [ -n "$BUILD_COMMITS_SINCE" ]; then
            # If there is build info, it means we are building an unofficial version (e.g. it does not have a git tag)
            # Bump version automatically, and use the build info to mark it as a pre-release version
            #
            # Note: Use a tilda (~) to seperate the version from the build info
            # as debian treats everything after ~ as a pre-release of the currently named
            # version.
            #
            # Example: 0.11.0~296-ga8025ffd should be considered less than 0.11.0
            #  Correct: dpkg --compare-versions 0.11.0~296-ga8025ffd '<<' 0.11.0
            #  Wrong: dpkg --compare-versions 0.11.0~296-ga8025ffd '<<' 0.11.0
            #
            # Example: 0.11.0~296-ga8025ffd should be less than 0.11.0~297-gabcdefab
            #
            AUTO_BUMP="patch"
            NEXT_BASE_VERSION=$(next_base_version "$BASE_VERSION" "$AUTO_BUMP")
            GIT_DESCRIBE="${NEXT_BASE_VERSION}~${BUILD_COMMITS_SINCE}+${BUILD_COMMIT_HASH}"
        else
            GIT_DESCRIBE="$BASE_VERSION"
        fi

        # only match if it looks like a semver version
        if [[ "$GIT_DESCRIBE" =~ ^[0-9]+\.[0-9]+\.[0-9]+.*$ ]]; then
            GIT_SEMVER="$GIT_DESCRIBE"
            echo "Using version set from git: $GIT_SEMVER" >&2
        else
            echo "git version does not match. got=$GIT_DESCRIBE, expected=^[0-9]+\.[0-9]+\.[0-9]+.*$" >&2
        fi
    else
        echo "git is not present on system. version will be handled by cargo directly" >&2
    fi
else
    echo "Using version set by user: $GIT_SEMVER" >&2
fi

# Only show version (for usage with other tooling)
if [ "$SHOW_VERSION" == "1" ]; then
    echo "$GIT_SEMVER"
    exit 0
fi

# Use zig to build as it is provides better cross compiling support
if ! cargo zigbuild --help &>/dev/null; then
    cargo install cargo-zigbuild
fi

# Allow users to install zig by other package managers
if ! zig --help &>/dev/null; then
    if ! python3 -m ziglang --help &>/dev/null; then
        # Fix version, as 0.11.0 does not build
        pip3 install ziglang==0.10.1.post1
    fi
fi

# Display zig version to help with debugging
echo "zig version: $(zig version ||:)"

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

# Load the release package list as $RELEASE_PACKAGES and $TEST_PACKAGES
# shellcheck disable=SC1091
source ./ci/package_list.sh

# build release for target
# GIT_SEMVER should be referenced in the build.rs scripts
if [ "$BUILD" = 1 ]; then
    cargo zigbuild "${TARGET[@]}" "${BUILD_OPTIONS[@]}"
fi

# Create release packages
OUTPUT_DIR="target/$ARCH/packages"

# Remove deprecated debian folder to avoid confusion with newly built linux packages
if [ -d "target/$ARCH/debian" ]; then
    echo "Removing deprecated debian folder created by cargo-deb: target/$ARCH/debian" >&2
    rm -rf "target/$ARCH/debian"
fi

./ci/build_scripts/package.sh build "$ARCH" "${RELEASE_PACKAGES[@]}" --version "$GIT_SEMVER" --output "$OUTPUT_DIR"

if [ "$BUILD" = 1 ]; then
    # Strip and build for test artifacts
    for PACKAGE in "${TEST_PACKAGES[@]}"
    do
        cargo zigbuild --release -p "$PACKAGE" "${TARGET[@]}"
    done
fi

# Package test binaries (deb only)
./ci/build_scripts/package.sh build "$ARCH" "${TEST_PACKAGES[@]}" --version "$GIT_SEMVER" --types deb --output "$OUTPUT_DIR" --no-clean
