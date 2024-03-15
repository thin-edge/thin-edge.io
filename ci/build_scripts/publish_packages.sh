#!/bin/bash
set -e

# enable debugging  by default in ci
if [ "$CI" = "true" ]; then
    set -x
fi

help() {
  cat <<EOF
Publish packages from a path to an external debian repository

All the necessary dependencies will be downloaded automatically if they are not already present

* Debian (deb)
* RPM (rmp)
* Alpine (apk)
* Tarball (tar.gz)

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
        --path ./target/packages

    \$ Publish all debian packages found under ./target/packages to the given repo


    $0 \\
        --path ./target/armv7-unknown-linux-gnueabihf/packages/

    \$ Publish all packages under ./target/packages
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
PUBLISH_REPO="${PUBLISH_REPO:-tedge-main}"
PUBLISH_DISTRIBUTION="${PUBLISH_DISTRIBUTION:-any-distro}"
PUBLISH_DISTRIBUTION_VERSION="${PUBLISH_DISTRIBUTION_VERSION:-any-version}"
PUBLISH_COMPONENT="${PUBLISH_COMPONENT:-main}"

CLOUDSMITH_COMMON_ARGS=()

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

        # Dry run. Don't upload anything, but validate everything else (still requires a valid token)
        --dry)
            CLOUDSMITH_COMMON_ARGS+=(
                "--dry-run"
            )
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


echo "---------------details-------------------------------"
echo "PUBLISH_OWNER:                   $PUBLISH_OWNER"
echo "PUBLISH_REPO:                    $PUBLISH_REPO"
echo "PUBLISH_DISTRIBUTION:            $PUBLISH_DISTRIBUTION"
echo "PUBLISH_DISTRIBUTION_VERSION:    $PUBLISH_DISTRIBUTION_VERSION"
echo "PUBLISH_COMPONENT:               $PUBLISH_COMPONENT"
echo "-----------------------------------------------------"

get_user_friendly_arch() {
    # Get an easy to use cpu architecture
    # which is easier for users to remember, and it also falls inline
    # with the docker CPU architecture (well the major ones at least)
    easy_arch=
    case "$1" in
        *x86_64-unknown-linux-*)
            easy_arch=amd64
            ;;
        *aarch64-unknown-linux-*)
            easy_arch=arm64
            ;;
        *armv7-unknown-linux-musleabihf*)
            easy_arch=armv7
            ;;
        *arm-unknown-linux-musleabihf*)
            easy_arch=armv6
            ;;
        *arm-unknown-linux-musleabi*)
            easy_arch=armv5
            ;;
        *armv5te-unknown-linux-*)
            easy_arch=armv5
            ;;
        *i686-unknown-linux-musl*)
            easy_arch=i386
            ;;
        *riscv64gc-unknown-linux-*)
            easy_arch=riscv64
            ;;
        *aarch64-apple-darwin*)
            easy_arch=macos-arm64
            ;;
        *x86_64-apple-darwin*)
            easy_arch=macos-amd64
            ;;
        *)
            echo "Unknown architecture. $1" >&2
            exit 1
            ;;
    esac
    echo "$easy_arch"
}

read_name_from_file() {
    #
    # Detect the package name from a file
    # e.g. output/tedge-openrc_0.0.0~rc0.tar.gz => tedge-openrc
    #
    name="$(basename "$1")"
    echo "Reading name from file: $name" >&2
    case "$name" in
        *.tar.gz)
            echo "${name%.tar.gz}" | cut -d'_' -f1
            ;;
        *)
            echo "${name%.*}" | cut -d'_' -f1
            ;;
    esac
}

read_version_from_file() {
    #
    # Detect the package version from a file
    # e.g. output/tedge-openrc_0.0.0~rc0.tar.gz => 0.0.0~rc0
    #
    name="$(basename "$1")"
    echo "Reading version from file: $name" >&2
    case "$name" in
        *_*)
            echo "${name%.*}" | sed 's/.tar$//g' | cut -d'_' -f2
            ;;
    esac
}

publish_linux() {
    # Publish matching debian packages to a debian repository
    #
    # Usage:
    #   publish <source_dir> <package_type> <pattern> <upload_path>
    #
    if [ $# -lt 4 ]; then
        echo "Invalid number of arguments. Expected at least 4 arguments" >&2
        echo "Function Usage: "
        echo "  publish <source_dir> <package_type> <pattern> <upload_path>"
        exit 1
    fi

    local sourcedir="$1"
    local package_type="$2"
    local pattern="$3"
    local sub_path="$4"

    local upload_path="${PUBLISH_OWNER}/${PUBLISH_REPO}"
    if [ -n "$sub_path" ]; then
        upload_path="$upload_path/$sub_path"
    fi

    if [ -z "$pattern" ]; then
        echo "Invalid pattern. Pattern must not be empty" >&2
        exit 1
    fi
    if [ -z "$package_type" ]; then
        echo "Invalid package type. package_type must not be empty" >&2
        exit 1
    fi

    # Notes: Currently Cloudsmith does not support the following (this might change in the future)
    #  * distribution and distribution_version must be selected from values in the list. use `cloudsmith list distros` to get the list
    #  * The component can not be set and is currently fixed to 'main'
    find "$sourcedir" -name "$pattern" -print0 | while read -r -d $'\0' file
    do
        cloudsmith upload "$package_type" "$upload_path" "$file" \
            --no-wait-for-sync \
            --api-key "${PUBLISH_TOKEN}" \
            "${CLOUDSMITH_COMMON_ARGS[@]}"
    done
}

publish_raw() {
    #
    # Publish a raw packages (e.g. a tarball)
    #
    local sourcedir="$1"
    local pattern="$2"
    local version="$3"
    local upload_path="${PUBLISH_OWNER}/${PUBLISH_REPO}"

    find "$sourcedir" -name "$pattern" -print0 | while read -r -d $'\0' file
    do
        # parse package info from filename
        pkg_name=$(read_name_from_file "$file")
        pkg_arch=$(get_user_friendly_arch "$file")
        pkg_version="${version:-}"
        if [ -z "$pkg_version" ]; then
            pkg_version=$(read_version_from_file "$file")
        fi

        if [ -z "$pkg_name" ]; then
            echo "Could not detect package name from file. file=$file" >&2
            exit 1
        fi

        if [ -z "$pkg_version" ]; then
            echo "Could not detect package version from file. file=$file" >&2
            exit 1
        fi

        # Create tmp package without the version information
        # so that the latest url is static.
        # Also use a file name which does not have the target architecture
        # so that it is easier to extract.
        mkdir -p tmp
        tmp_file="tmp/${pkg_name}.tar.gz"
        cp "$file" "$tmp_file"

        # Include package architecture in the name
        # to avoid conflicts between the different architectures
        # and what is the "latest" package. Default to using the name
        # if there is no architecture (to be generic)
        full_pkg_name="$pkg_name"
        if [ -n "$pkg_arch" ]; then
            full_pkg_name="${pkg_name}-${pkg_arch}"
        fi

        echo "Uploading file: $file (name=$full_pkg_name, version=$pkg_version, file=$tmp_file)"
        cloudsmith upload raw "$upload_path" "$tmp_file" \
            --name "$full_pkg_name" \
            --version "$pkg_version" \
            --no-wait-for-sync \
            --api-key "${PUBLISH_TOKEN}" \
            "${CLOUDSMITH_COMMON_ARGS[@]}"

        rm -rf tmp
    done
}

publish_raw "$SOURCE_PATH" "*.tar.gz"

publish_linux "$SOURCE_PATH" "deb" "*.deb" "any-distro/any-version"
publish_linux "$SOURCE_PATH" "rpm" "*.rpm" "any-distro/any-version"
publish_linux "$SOURCE_PATH" "alpine" "*.apk" "alpine/any-version"
