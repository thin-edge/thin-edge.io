#!/usr/bin/env bash
set -e
DELETE=0

usage() {
    cat <<EOT
Find and delete unused (unreferenced) images (e.g. .png) in the project

$0 [--delete]

ARGUMENTS

  --delete      Delete the image if it is not referenced anywhere

EXAMPLES

$0 
# List images which are not referenced anywhere (but don't delete them)

$0 --delete
# Find and delete images which are not referenced anywhere

EOT
}

while [ $# -gt 0 ]; do
    case "$1" in
        --delete)
            DELETE=1
            ;;
        --help|-h)
            usage
            exit 0
            ;;
    esac
    shift
done

if ! command -V rg >/dev/null 2>&1; then
    echo "Could not find rg (ripgrep). Please install and try again" >&2
    exit 1
fi

TOTAL_REMOVED=0

for line in $(rg --files | rg .png | awk -F '/' '{print $NF}'); do
    if ! rg "$line" . >/dev/null; then
        TOTAL_REMOVED=$((TOTAL_REMOVED + 1))
        echo "No references for: $line" >&2

        if [ "$DELETE" = 1 ]; then
            find . -name "$line" -delete
        fi
    fi
done

case "$TOTAL_REMOVED" in
    0)
        echo "No unused images found" >&2
        ;;
    1)
        echo "Found $TOTAL_REMOVED unused image found" >&2
        if [ "$DELETE" = 0 ]; then
            exit 1
        fi
        ;;
    *)
        echo "Found $TOTAL_REMOVED unused images found" >&2
        if [ "$DELETE" = 0 ]; then
            exit 1
        fi
        ;;
esac

exit 0
