#!/bin/bash
set -e

help() {
  cat <<EOF
Publish debian packages from a path to an external debian repository

All the necessary dependencies will be downloaded automatically if they are not already present

Usage:
    $0

Flags:
    --token <string>            Debian access token used to authenticate the commands
    --owner <string>            Debian repository owner, e.g. thinedge
    --repo <string>             Name of the debian repository to publish to, e.g. buster, or stable
    --distribution <string>     Name of the debian distribution to publish to, e.g. stable, raspbian. Defaults to stable
    --component <string>        Currently not supported, waiting for cloudsmith api to support it! Name of the debian component to publish to, e.g. main, unstable etc. Defaults to main.
    --help|-h                   Show this help

Optional Environment variables (instead of flags)

PUBLISH_TOKEN            Equivalent to --token flag
PUBLISH_OWNER            Equivalent to --owner flag
PUBLISH_REPO             Equivalent to --repo flag
PUBLISH_DISTRIBUTION     Equivalent to --distribution flag
PUBLISH_COMPONENT        Equivalent to --component flag

Examples:
    $0 \\
        --token "mywonderfultoken" \\
        --repo "tedge-main" \\
        --path ./target/debian

    \$ Publish all debian packages found under ./target/debian to the given repo


    $0 \\
        --path ./target/armv7-unknown-linux-gnueabihf/debian/

    \$ Publish all debian packages under ./target/debian but group them in the debian pool, so they are easier to manage
EOF
}

# Add local tools path
LOCAL_TOOLS_PATH="$HOME/.local/bin"
export PATH="$LOCAL_TOOLS_PATH:$PATH"

# Install tooling if missing
if ! [ -x "$(command -v cloudsmith)" ]; then
    echo 'Install cloudsmith cli' >&2
    if command -v pip3 &>/dev/null; then
        pip3 install --upgrade cloudsmith-cli
    elif command -v pip &>/dev/null; then
        pip install --upgrade cloudsmith-cli
    else
        echo "Could not install cloudsmith cli. Reason: pip3/pip is not installed"
        exit 2
    fi
fi

# Disable prompting
export CI=true

# Enable setting values via env variables (easier for CI for secrets)
PUBLISH_TOKEN="${PUBLISH_TOKEN:-}"
PUBLISH_OWNER="${PUBLISH_OWNER:-thinedge}"
PUBLISH_REPO="${PUBLISH_REPO:-}"
PUBLISH_DISTRIBUTION="${PUBLISH_DISTRIBUTION:-any-distro}"
PUBLISH_DISTRIBUTION_VERSION="${PUBLISH_DISTRIBUTION_VERSION:-any-version}"
PUBLISH_COMPONENT="${PUBLISH_COMPONENT:-main}"

#
# Argument parsing
#
POSITIONAL=()
while [[ $# -gt 0 ]]
do
    case "$1" in
        # Repository owner
        --owner)
            PUBLISH_OWNER="$2"
            shift
            ;;

        # Token used to authenticate publishing commands
        --token)
            PUBLISH_TOKEN="$2"
            shift
            ;;

        # Where to look for the debian files to publish
        --path)
            SOURCE_PATH="$2"
            shift
            ;;

        # Which debian repo to publish to (under the given host url)
        --repo)
            PUBLISH_REPO="$2"
            shift
            ;;

        # Which Debian distribution to publish to
        --distribution)
            PUBLISH_DISTRIBUTION="$2"
            shift
            ;;

        --distribution-version)
            PUBLISH_DISTRIBUTION_VERSION="$2"
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

RUST_TUPLE="arm-unknown-linux-gnueabihf"

case "$RUST_TUPLE" in
    arm-unknown-linux-gnueabihf)

        ;;
esac


echo "---------------details-------------------------------"
echo "PUBLISH_OWNER:                   $PUBLISH_OWNER"
echo "PUBLISH_REPO:                    $PUBLISH_REPO"
echo "PUBLISH_DISTRIBUTION:            $PUBLISH_DISTRIBUTION"
echo "PUBLISH_DISTRIBUTION_VERSION:    $PUBLISH_DISTRIBUTION_VERSION"
echo "PUBLISH_COMPONENT:               $PUBLISH_COMPONENT"
echo "-----------------------------------------------------"

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
    local distribution_version="$1"
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

    # Notes: Currently Cloudsmith does not support the following (this might change in the future)
    #  * distribution and distribution_version must be selected from values in the list. use `cloudsmith list distros` to get the list
    #  * The component can not be set and is currently fixed to 'main'
    find "${SOURCE_PATH}" -name "${pattern}_${arch}.deb" -print0 | while read -r -d $'\0' file
    do
        cloudsmith upload deb "${PUBLISH_OWNER}/${PUBLISH_REPO}/${distribution}/${distribution_version}" "$file" \
            --no-wait-for-sync \
            --api-key "${PUBLISH_TOKEN}"
    done
}

publish_for_distribution() {
    # Publish debian packages for all given architectures to a specific repository distribution
    # Usage:
    #   publish_for_distribution <distribution> <distribution_version> <arch> [arch...]
    #
    local distribution="$1"
    shift
    local distribution_version="$1"
    shift
    for arch in "$@"
    do
        echo "[distribution=$distribution, arch=$arch] Publishing packages"
        publish "$distribution" "$distribution_version" "**" "$arch"
    done
}

publish_for_distribution "$PUBLISH_DISTRIBUTION" "$PUBLISH_DISTRIBUTION_VERSION" "${ARCHITECTURES[@]}" "all"