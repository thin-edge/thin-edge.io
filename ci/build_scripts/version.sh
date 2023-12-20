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
* CONTAINER_VERSION
* TARBALL_VERSION

NOTES

If you have previously sourced the script into your current shell environment then you will need to unset the GIT_SEMVER variable otherwise the previously created GIT_SEMVER value will be used.
Alternatively, you can limit to calling the script only from other scripts to avoid leakage of the environment variables between script calls.

Example:
    unset GIT_SEMVER
    . $0

USAGE
    $0 [apk|deb|rpm|container|tarball|all]
    # Print out a version

    # importing values via a script
    . $0 [--version <version>]

EXAMPLES

. $0
# Export env variables for use in packaging

unset GIT_SEMVER
. $0
# Export env variables for use in package but ignore any previously set value

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
    patch=$(echo "$version" | cut -d'.' -f3 | cut -d- -f1)

    # If a release candidate version, then only increment to the
    # next release candidate, e.g. 1.0.0-rc.1 -> 1.0.0-rc.2
    if [[ "$version" =~ -rc\.[0-9]+ ]]; then
        rc=$(echo "$version" | cut -d'-' -f2 | tr -d a-zA-Z.)
        if [ -n "$rc" ]; then
            rc=$((rc + 1))
            echo "${major}.${minor}.${patch}-rc.${rc}"
            return
        fi
    fi

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

set_version_variables() {

    BUILD_COMMITS_SINCE=
    BUILD_COMMIT_HASH=
    BASE_VERSION=
    BUMP_VERSION=0

    if [ -z "$GIT_SEMVER" ]; then
        GIT_DESCRIBE_RAW=$(git describe --always --tags --abbrev=7 2>/dev/null || true)

        if [[ "$GIT_DESCRIBE_RAW" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            # Tagged release
            BASE_VERSION="$GIT_DESCRIBE_RAW"
        elif [[ "$GIT_DESCRIBE_RAW" =~ ^[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9]+$ ]]; then
            # Pre-release tagged release, e.g. 1.0.0-rc.1
            BASE_VERSION="$GIT_DESCRIBE_RAW"
        elif [[ "$GIT_DESCRIBE_RAW" =~ ^[a-z0-9]+$ ]]; then
            # Note: Sometimes git describe only prints out the git hash when run on a PR branch
            # from someone else. In such instances this causes the version to be incompatible with
            # linux package types. For instance, debian versions must start with a digit.
            # When this situation is detected, git describe is run on the main branch however the
            # git hash is replaced with the current git hash of the current branch.
            echo "Using git describe from origin/main" >&2
            BUILD_COMMIT_HASH="g$GIT_DESCRIBE_RAW"
            GIT_DESCRIBE_RAW=$(git describe --always --tags --abbrev=7 origin/main 2>/dev/null || true)
            BASE_VERSION=$(echo "$GIT_DESCRIBE_RAW" | cut -d- -f1)
            BUILD_COMMITS_SINCE=$(echo "$GIT_DESCRIBE_RAW" | cut -d- -f2)
        else
            BASE_VERSION=$(echo "$GIT_DESCRIBE_RAW" | sed -E 's|-[0-9]+-g[a-f0-9]+$||g')
            BUILD_COMMITS_SINCE=$(echo "$GIT_DESCRIBE_RAW" | rev | cut -d- -f2 | rev)
            BUILD_COMMIT_HASH=$(echo "$GIT_DESCRIBE_RAW" | rev | cut -d- -f1 | rev)
        fi
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
    # Check the version is already marked as a release candidate
    if [[ "$GIT_SEMVER" = *-rc* ]]; then
        APK_VERSION="${GIT_SEMVER//\~/-}"
        # Replace first - with an _ as apk expects that the release candidate information is
        # separated by an underscore
        # shellcheck disable=SC2001
        APK_VERSION=$(echo "$APK_VERSION" | sed 's/-/_/')
    else
        APK_VERSION="${GIT_SEMVER//\~/_rc}"
    fi
    DEB_VERSION="$GIT_SEMVER"
    RPM_VERSION="$GIT_SEMVER"
    # container tags are quite limited, so replace forbidden characters with '-'
    CONTAINER_VERSION="${GIT_SEMVER//[^a-zA-Z0-9_.-]/-}"

    # Check the version is already marked as a release candidate
    if [[ "$GIT_SEMVER" = *-rc* ]]; then
        TARBALL_VERSION="${GIT_SEMVER//\~/-}"
    else
        TARBALL_VERSION="${GIT_SEMVER//\~/-rc}"
    fi

    export GIT_SEMVER
    export APK_VERSION
    export DEB_VERSION
    export RPM_VERSION
    export CONTAINER_VERSION
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
        tarball)
            echo "$TARBALL_VERSION"
            ;;
        container)
            echo "$CONTAINER_VERSION"
            ;;
        all)
            echo "GIT_SEMVER: $GIT_SEMVER"
            echo "APK_VERSION: $APK_VERSION"
            echo "DEB_VERSION: $DEB_VERSION"
            echo "RPM_VERSION: $RPM_VERSION"
            echo "CONTAINER_VERSION: $CONTAINER_VERSION"
            echo "TARBALL_VERSION: $TARBALL_VERSION"
            ;;
        *)
            echo "$GIT_SEMVER"
            ;;
    esac
fi
