#!/usr/bin/env bash
set -ex

usage() {
    cat << EOT
Generate release notes between two different versions

$0
$0 [FROM_VERSION] [TO_VERSION]


Flags
    --from-tags                     Detect from/to versions are detected
                                    from the most recent two git tags (version sorted)
    --help | -h                     Show this help

Examples

    $0
    # Generate the changelog since the last release

    $0 --from-tags
    # Generate the changelog from the latest official release (detected from git tags)

    $0 1.1.1 HEAD
    # Generate the changelog between version 1.1.1 and the current unreleased version (using the new version)

    $0 1.0.0 1.1.0
    # Generate the changelog between two existing tags
EOT
}

FROM_VERSION=
TO_VERSION=
FROM_TAGS=0

while [ $# -gt 0 ]; do
    case "$1" in
        --help|-h)
            usage
            exit 0
            ;;
        --from-tags)
            FROM_TAGS=1
            ;;
        --*|-*)
            echo "Unknown flag" >&2
            usage
            exit 1
            ;;
        *)
            if [ -z "$FROM_VERSION" ]; then
                FROM_VERSION="$1"
            elif [ -z "$TO_VERSION" ]; then
                TO_VERSION="$1"
            else
                echo "Unexpected positional argument" >&2
                usage
                exit 1
            fi
            ;;
    esac
    shift
done

# install dependency
if ! command -v git-cliff >/dev/null 2>&1; then
    cargo install git-cliff
fi

if ! command -v jq >/dev/null 2>&1; then
    echo "missing required dependency: jq" >&2
    exit 1
fi

if [ -z "$GITHUB_TOKEN" ]; then
    echo "Warning: You haven't set the GITHUB_TOKEN environment variable. The changelog generation might fail as Github rate limits unauthenticated users" >&2
    echo "Set your token using: " >&2
    echo >&2
    echo "   export GITHUB_TOKEN=$(gh auth token)"
    echo >&2
fi

# Lookup the from/to versions using git --tag (with version sorting)
if [ "$FROM_TAGS" = 1 ]; then
    echo "Detecting from/to version from git tags" >&2
    FROM_VERSION=$(git tag --list | sort -V | grep "^[0-9]" | tail -n2 | head -n1)
    TO_VERSION=$(git tag --list | sort -V | grep "^[0-9]" | tail -n1)
fi

if [ -z "$FROM_VERSION" ]; then
    FROM_VERSION=$(git-cliff --context --unreleased | jq -r '.[0].previous.version')
fi

if [ -z "$TO_VERSION" ]; then
    TO_VERSION=$(git-cliff --context --unreleased | jq -r '.[0].version // "HEAD"')
fi

CLIFF_ARGS=()

if [ "$TO_VERSION" = "HEAD" ]; then
    # Calculate next version
    VERSION=$(git-cliff --bumped-version)
    CLIFF_ARGS+=(
        --tag "$VERSION"
    )
fi

git-cliff "${CLIFF_ARGS[@]}" "$FROM_VERSION".."$TO_VERSION" --output _CHANGELOG.md
