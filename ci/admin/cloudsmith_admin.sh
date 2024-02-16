#!/usr/bin/env bash
set -e

help() {
  cat <<EOF
Cloudsmith admin script to perform cleanups on the repositories

This script is not intended for common use, and should only be executed by an admin!

Usage:
    $0 <ACTION>

Args:
    ACTION     Which action to execute

ACTION

    cleanup     Remove old versions from the tedge-main and tedge-main-armv6 repositories which
                where uploaded longer than x days ago.
    
    promote     Promote already published packages from the dev repo to the release repo

Env:
    PUBLISH_TOKEN       Cloudsmith API token used for authorize the delete commands

Flags:
    --help|-h   Show this help

Examples:
    $0 cleanup  
    # Remove old versions
EOF
}

REST_ARGS=()
while [ $# -gt 0 ]; do
    case "$1" in
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

if [ $# -lt 1 ]; then
    echo "Missing argument" >&2
    exit 1
fi

COMMAND="$1"
shift

COMMON_ARGS=()

if [ -n "$PUBLISH_TOKEN" ]; then
    COMMON_ARGS+=(
        --api-key "${PUBLISH_TOKEN}"
    )
fi

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

#
# Functions
#
delete_by_version() {
    if [ $# -lt 1 ]; then
        echo "Missing version. Please provide the version to delete as the first positional argument" >&2
        exit 1
    fi
    version="$1"
    cloudsmith ls pkg thinedge/tedge-main -q "version:*$version*" -F json -l 100 \
    | jq '.data[] | .namespace + "/" + .repository + "/" + .slug' -r \
    | xargs -Ipackage cloudsmith delete package --yes "${COMMON_ARGS[@]}"

    cloudsmith ls pkg thinedge/tedge-main-armv6 -q "version:*$version*" -F json -l 100 \
    | jq '.data[] | .namespace + "/" + .repository + "/" + .slug' -r \
    | xargs -Ipackage cloudsmith delete package --yes "${COMMON_ARGS[@]}"
}

list_old_versions() {
    # Only use the "tedge" package to determine the versions
    # Version filter: Only included versions with "g" in it, as it identifies non-official versions e.g. 1.0.0~rc.2~51+g1234abc
    # Uploaded filter: Only filter for packages uploaded that are older than x days ago
    cloudsmith ls pkg thinedge/tedge-main -q "format:deb AND name:^tedge$ AND version:g AND architecture:arm64 AND uploaded:<'60 days ago'" -l 500 -F json \
    | jq -r '.data[] | .version'
}

delete_old_versions() {
    while read -r version; do
        # Get version commit id from the full version string
        # as it is common between the different package variants, e.g. rpm, apk, deb, tar
        # Using the full version would not work due to differences between the version formatting
        version_commit=$(echo "$version" | cut -d'+' -f2)
        if [ -n "$version_commit" ]; then
            echo "Removing old version: $version, commit=$version_commit"
            delete_by_version "$version_commit"
        fi
    done < <(list_old_versions)
}

promote_version() {
    #
    # Promote version from the main repo to the official release repo
    #

    # Get the versions
    # shellcheck disable=SC1091
    . ./ci/build_scripts/version.sh all --version "$1"

    if [ -z "$APK_VERSION" ]; then
        echo "APK_VERSION variable is empty" >&2
        return 1
    fi
    if [ -z "$RPM_VERSION" ]; then
        echo "RPM_VERSION variable is empty" >&2
        return 1
    fi
    if [ -z "$DEB_VERSION" ]; then
        echo "DEB_VERSION variable is empty" >&2
        return 1
    fi
    if [ -z "$TARBALL_VERSION" ]; then
        echo "TARBALL_VERSION variable is empty" >&2
        return 1
    fi

    # Build cloudsmith query using the different package version variants
    APK_VERSION_QUERY="(version:^${APK_VERSION}-r0\$ AND format:alpine)"
    RPM_VERSION_QUERY="(version:^${RPM_VERSION}-1\$ AND format:rpm)"
    DEB_VERSION_QUERY="(version:^${DEB_VERSION}\$ AND format:deb)"
    TARBALL_VERSION_QUERY="(version:^${TARBALL_VERSION}\$ AND format:raw)"
    query="$APK_VERSION_QUERY OR $RPM_VERSION_QUERY OR $DEB_VERSION_QUERY OR $TARBALL_VERSION_QUERY"

    if [ -z "$query" ]; then
        echo "Unknown package query"
        exit 1
    fi

    printf "Cloudsmith package selection query:\n    %s\n\n" "$query" >&2

    cloudsmith ls pkg thinedge/tedge-main -q "$query" -F json -l 500 \
    | jq '.data[] | .namespace + "/" + .repository + "/" + .slug' -r \
    | xargs -Ipackage cloudsmith cp package thin-edge/tedge-release "${COMMON_ARGS[@]}"

    cloudsmith ls pkg thinedge/tedge-main-armv6 -q "$query" -F json -l 500 \
    | jq '.data[] | .namespace + "/" + .repository + "/" + .slug' -r \
    | xargs -Ipackage cloudsmith cp package thin-edge/tedge-release-armv6 "${COMMON_ARGS[@]}"
}

#
# Main
#
case "$COMMAND" in
    cleanup)
        delete_old_versions
        ;;
    promote)
        if [ $# -lt 1 ]; then
            echo "Missing version. Please provide the version as the first positional argument" >&2
            exit 1
        fi
        promote_version "$1"
        ;;
    *)
        echo "Unknown action" >&2
        help
        exit 1
        ;;
esac
