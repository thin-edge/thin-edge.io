#!/bin/sh
set -e

TEDGE_CONFIG_DIR=${TEDGE_CONFIG_DIR:-/etc/tedge}
COMMAND=""

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --config-dir)
            TEDGE_CONFIG_DIR="$2"
            shift 2
            ;;
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
    if command -V tedge > /dev/null 2>&1; then
        echo "tedge http get /te/v1/entities --config-dir $TEDGE_CONFIG_DIR"
            if command -V jq > /dev/null 2>&1; then
                tedge http get /te/v1/entities --config-dir "$TEDGE_CONFIG_DIR" | jq
            else
                tedge http get /te/v1/entities --config-dir "$TEDGE_CONFIG_DIR"
            fi
    fi
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
