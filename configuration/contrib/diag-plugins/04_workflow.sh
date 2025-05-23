#!/bin/sh
set -e

OUTPUT_DIR=""
COMMAND=""
LOGS_PATH="$(tedge config get logs.path)"

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --output-dir)
            OUTPUT_DIR="$2"
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
    if [ -d "$LOGS_PATH"/agent ]; then
        for file in "$LOGS_PATH"/agent/*; do
            cp "$file" "$OUTPUT_DIR"/
        done
    else 
        echo "${LOGS_PATH} not found" >&2
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
