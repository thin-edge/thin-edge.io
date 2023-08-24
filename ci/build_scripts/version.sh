#!/bin/bash
set -e

usage() {
    cat <<EOT
Set version variables for various different packaging formats (e.g. apk (Alpine Linux), deb (Debian), rpm (RHEL/Fedora))

The following environment variables are set:

* GIT_SEMVER
* APK_VERSION
* DEB_VERSION
* RPM_VERSION

USAGE
    $0 [apk|deb|rpm|all]

    # importing values via a script
    . $0 [--version <version>]

EXAMPLES

. $0
# Export env variables for use in packaging

. $0 --version 1.2.3
# Export env variables but use an explicit value

EOT
}

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

parse_version() {
    git describe --always --tags --abbrev=7 2>/dev/null || true
}

set_version_variables() {

    BUILD_COMMITS_SINCE=
    BUILD_COMMIT_HASH=
    BASE_VERSION=
    BUMP_VERSION=0

    if [ -z "$GIT_SEMVER" ]; then
        GIT_DESCRIBE_RAW=$(git describe --always --tags --abbrev=7 2>/dev/null || true)
        BASE_VERSION=$(echo "$GIT_DESCRIBE_RAW" | cut -d- -f1)
        BUILD_COMMITS_SINCE=$(echo "$GIT_DESCRIBE_RAW" | cut -d- -f2)
        BUILD_COMMIT_HASH=$(echo "$GIT_DESCRIBE_RAW" | cut -d- -f3)
        BUMP_VERSION=1
    else
        echo "Using version set by user: $GIT_SEMVER" >&2
        if echo "$GIT_SEMVER" | grep -Eq '.+~.+\+.+'; then
            BASE_VERSION=$(echo "$GIT_SEMVER" | cut -d'~' -f1)
            VERSION_META=$(echo "$GIT_SEMVER" | cut -d'~' -f2)
            BUILD_COMMITS_SINCE=$(echo "$VERSION_META" | cut -d'+' -f1)
            BUILD_COMMIT_HASH=$(echo "$VERSION_META" | cut -d'+' -f2)
        else
            BASE_VERSION="$GIT_SEMVER"
        fi
    fi

    if [ -n "$BUILD_COMMITS_SINCE" ]; then
        # If there is build info, it means we are building an unofficial version (e.g. it does not have a git tag)
        # Bump version automatically, and use the build info to mark it as a pre-release version
        #
        NEXT_BASE_VERSION="$BASE_VERSION"
        if [ "$BUMP_VERSION" = '1' ]; then
            AUTO_BUMP="patch"
            NEXT_BASE_VERSION=$(next_base_version "$BASE_VERSION" "$AUTO_BUMP")
        fi
        GIT_SEMVER="${NEXT_BASE_VERSION}~${BUILD_COMMITS_SINCE}+${BUILD_COMMIT_HASH}"
    else
        GIT_SEMVER="$BASE_VERSION"
    fi

    # Alpine does not accepts a tilda, it needs "_rc" instead
    # https://wiki.alpinelinux.org/wiki/APKBUILD_Reference#pkgver
    APK_VERSION="${GIT_SEMVER//\~/_rc}"
    DEB_VERSION="$GIT_SEMVER"
    RPM_VERSION="$GIT_SEMVER"
    TARBALL_VERSION="${GIT_SEMVER//\~/-rc}"

    export GIT_SEMVER
    export APK_VERSION
    export DEB_VERSION
    export RPM_VERSION
    export TARBALL_VERSION
}


test_debian() {
    current="1.0.0";
    version_seq=(
        "1.0.1~1+g9999999"
        "1.0.1~99+g8888888"
        "1.0.1~100+g7777777"
        "1.0.1"
    )
    for next in "${version_seq[@]}"; do
        if dpkg --compare-versions "$next" ">>" "$current"; then
            echo "PASS: $next > $current" >&2;
        else
            echo "FAIL: expected $next to be > $current" >&2;
        fi
        current="$next";
    done
}

test_alpine() {
    current="1.0.0";
    version_seq=(
        "1.0.1_rc1+g9999999"
        "1.0.1_rc99+g8888888"
        "1.0.1_rc100+g7777777"
        "1.0.1"
    )
    for next in "${version_seq[@]}"; do
        if [ "$(apk version -t "$current" "$next")" = "<" ]; then
            echo "PASS: $next > $current" >&2;
        else
            echo "FAIL: expected $next to be > $current" >&2;
        fi
        current="$next";
    done
}

#
# Main
#
POSITIONAL=()
while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            if [ -n "$2" ]; then
                GIT_SEMVER="$2"
            fi
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        --*|-*)
            echo "Unknown flags: $1" >&2
            usage
            exit 1
            ;;
        *)
            POSITIONAL+=("$1")
            ;;
    esac
    shift
done

# Only set if rest arguments are defined
if [ ${#POSITIONAL[@]} -gt 0 ]; then
    set -- "${POSITIONAL[@]}"
fi

set_version_variables

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    # Script is not being sourced, so print the desired value
    case "$1" in
        apk)
            echo "$APK_VERSION"
            ;;
        deb)
            echo "$DEB_VERSION"
            ;;
        rpm)
            echo "$RPM_VERSION"
            ;;
        all)
            echo "GIT_SEMVER: $GIT_SEMVER"
            echo "APK_VERSION: $APK_VERSION"
            echo "DEB_VERSION: $DEB_VERSION"
            echo "RPM_VERSION: $RPM_VERSION"
            ;;
        *)
            echo "$GIT_SEMVER"
            ;;
    esac
fi
