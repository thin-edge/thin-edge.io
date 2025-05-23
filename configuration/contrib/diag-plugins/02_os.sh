#!/bin/sh
set -e

COMMAND=""

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        collect)
            COMMAND="collect"
            shift
            ;;
        *)
            shift
            ;;
    esac
done

collect() {
    if [ -f /etc/os-release ]; then
        echo "/etc/os-release"
        cat /etc/os-release
    fi

    echo "system information"
    uname -a
}


case "$COMMAND" in
    collect)
        collect
        ;;
    *)
        echo "Unknown command" >&2
        exit 1
        ;;
esac

exit 0
