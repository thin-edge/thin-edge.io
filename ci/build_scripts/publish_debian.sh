#!/bin/bash
set -e

help() {
  cat <<EOF
Publish debian packages from a path to an external debian repository

All the necessary dependencies will be downloaded automatically if they are not already present

Usage:
    $0

Flags:
    --url <string>              JFrog repository URL, e.g. https://myrepo.jfrog.io/artifactory
    --token <string>            JFrog access token used to authenticate the commands
    --repo <string>             Name of the debian repository to publish to, e.g. buster, or stable
    --distribution <string>     Name of the debian distribution to publish to, e.g. stable, raspbian. Defaults to stable
    --component <string>        Name of the debian component to publish to, e.g. main, unstable etc. Defaults to main
    --group <string>            Group name used to group the artifacts in a specific pool (this has no affect on the package itself)
    --path <string>             Path where the debian (.deb) files are located, e.g. ./target/debian
    --help|-h                   Show this help

Optional Environment variables (instead of flags)

PUBLISH_URL              Equivalent to --url flag
PUBLISH_TOKEN            Equivalent to --token flag
PUBLISH_REPO             Equivalent to --repo flag
PUBLISH_DISTRIBUTION     Equivalent to --distribution flag
PUBLISH_COMPONENT        Equivalent to --component flag
PUBLISH_POOL_GROUP       Equivalent to --group flag

Examples:
    $0 \\
        --url https://myrepo.jfrog.io/artifactory \\
        --token "mywonderfultoken" \\
        --repo "stable" \\
        --distribution "stable" \\
        --path ./target/debian

    \$ Publish all debian packages found under ./target/debian to the given Jfrog repo


    $0 \\
        --path ./target/armv7-unknown-linux-gnueabihf/debian/ \\
        --group 0.8.1-105-g6a5fdeee/armv7

    \$ Publish all debian packages under ./target/debian but group them in the debian pool, so they are easier to manage
EOF
}

# Add local tools path
LOCAL_TOOLS_PATH="$HOME/.local/bin"
export PATH="$LOCAL_TOOLS_PATH:$PATH"

# Install tooling if missing
if ! [ -x "$(command -v jfrog)" ]; then
    echo 'Install jfrog cli' >&2
    curl -fL https://getcli.jfrog.io/v2 | sh
    mkdir -p "$LOCAL_TOOLS_PATH"
    mv jfrog "$LOCAL_TOOLS_PATH/"
fi

# Disable jfrog prompting
export CI=true

# Enable setting values via env variables (easier for CI for secrets)
PUBLISH_URL="${PUBLISH_URL:-}"
PUBLISH_TOKEN="${PUBLISH_TOKEN:-}"
PUBLISH_REPO="${PUBLISH_REPO:-}"
PUBLISH_DISTRIBUTION="${PUBLISH_DISTRIBUTION:-stable}"
PUBLISH_POOL_GROUP="${PUBLISH_POOL_GROUP:-}"
PUBLISH_COMPONENT="${PUBLISH_COMPONENT:-main}"

#
# Argument parsing
#
POSITIONAL=()
while [[ $# -gt 0 ]]
do
    case "$1" in
        # Jfrog url
        --url)
            PUBLISH_URL="$2"
            ;;

        # Token used to authenticate jfrog commands
        --token)
            PUBLISH_TOKEN="$2"
            shift
            ;;

        # Where to look for the debian files to publish
        --path)
            SOURCE_PATH="$2"
            shift
            ;;

        # Extra path 
        --group)
            PUBLISH_POOL_GROUP="$2"
            shift
            ;;

        # Which jfrog repo to publish to (under the given jfrog url)
        --repo)
            PUBLISH_REPO="$2"
            shift
            ;;

        # Which Debian distribution to publish to
        --distribution)
            PUBLISH_DISTRIBUTION="$2"
            shift
            ;;

        # Which Debian component to publish to (accepts csv list)
        --component)
            PUBLISH_COMPONENT="$2"
            shift
            ;;

        --help|-h)
            help
            exit 0
            ;;
        
        -*)
            echo "Unrecognized flag" >&2
            help
            exit 1
            ;;

        *)
            POSITIONAL+=("$1")
            ;;
    esac
    shift
done
set -- "${POSITIONAL[@]}"

# Normalize pool group name (to prevent url errors)
PUBLISH_POOL_GROUP="${PUBLISH_POOL_GROUP//[^A-Za-z0-9-\/]/_}"
# trim any trailing slashes (if defined) to prevent double slash problems
if [ -n "$PUBLISH_POOL_GROUP" ]; then
    PUBLISH_POOL_GROUP="${PUBLISH_POOL_GROUP%/}/"
fi

RUST_TUPLE="arm-unknown-linux-gnueabihf"

case "$RUST_TUPLE" in
    arm-unknown-linux-gnueabihf)

        ;;
esac


echo "---------------details----------------------"
echo "PUBLISH_URL:             $PUBLISH_URL"
echo "PUBLISH_REPO:            $PUBLISH_REPO"
echo "PUBLISH_DISTRIBUTION:    $PUBLISH_DISTRIBUTION"
echo "PUBLISH_COMPONENT:       $PUBLISH_COMPONENT"
echo "PUBLISH_POOL_GROUP:      $PUBLISH_POOL_GROUP"
echo "--------------------------------------------"

ARCHITECTURES=(
    amd64
    arm64
    armhf
    armel
)

publish() {
    # Publish matching debian packages to a debian repository
    #
    # Usage:
    #   publish <distribution> <pattern> <arch> [arch...]
    #
    if [ $# -lt 2 ]; then
        echo "Invalid number of arguments. Expected at least 2 arguments" >&2
        exit 1
    fi

    local distribution="$1"
    shift
    local pattern="$1"
    shift
    local arch="$1"
    shift

    if [ -z "$pattern" ]; then
        echo "Invalid pattern. Pattern must not be empty" >&2
        exit 1
    fi

    if [ -z "$arch" ]; then
        echo "Invalid architecture. Architecture must not be empty" >&2
        exit 1
    fi

    jfrog rt upload \
        --url "${PUBLISH_URL}/${PUBLISH_REPO}" \
        --access-token "${PUBLISH_TOKEN}" \
        --deb "${distribution}/${PUBLISH_COMPONENT}/${arch}" \
        --flat \
        "${SOURCE_PATH}/${pattern}_${arch}.deb" \
        "/pool/${distribution}/${PUBLISH_POOL_GROUP}"
}

publish_for_distribution() {
    # Publish debian packages for all given architectures to a specific repository distribution
    # Usage:
    #   publish_for_distribution <distribution> <arch> [arch...]
    #
    local distribution="$1"
    shift
    for arch in "$@"
    do
        echo "[distribution=$distribution, arch=$arch] Publishing packages"
        publish "$distribution" "**" "$arch"
    done
}

publish_for_distribution "$PUBLISH_DISTRIBUTION" "${ARCHITECTURES[@]}" "all"
