#!/bin/sh
set -e

OUTPUT_DIR=""
TEDGE_CONFIG_DIR=${TEDGE_CONFIG_DIR:-/etc/tedge}
COMMAND=""

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
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

# Check if the output directory exists
if [ -n "$OUTPUT_DIR" ] && [ ! -d "$OUTPUT_DIR" ]; then
    echo "Error: Output directory does not exist: $OUTPUT_DIR" >&2
    exit 1
fi


collect() {
    LOGS_PATH=$(tedge config get logs.path --config-dir "$TEDGE_CONFIG_DIR")
    if [ -d "$LOGS_PATH"/agent ]; then
        find "$LOGS_PATH/agent" -type f -exec cp {} "$OUTPUT_DIR"/ \; ||:
    else
        echo "$LOGS_PATH/agent not found" >&2
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
