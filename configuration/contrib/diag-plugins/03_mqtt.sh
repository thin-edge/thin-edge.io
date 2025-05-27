#!/bin/sh
set -e

OUTPUT_DIR=""
COMMAND=""

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
    if command -V tedge > /dev/null 2>&1; then
        echo "tedge mqtt sub '#' --duration 5s" > "$OUTPUT_DIR"/tedge-mqtt-sub.log 2>&1
        tedge mqtt sub '#' --duration 5s >> "$OUTPUT_DIR"/tedge-mqtt-sub.log 2>&1
        echo "tedge mqtt sub '#' --duration 1s --retained-only" > "$OUTPUT_DIR"/tedge-mqtt-sub-retained-only.log 2>&1
        tedge mqtt sub '#' --duration 1s --retained-only >> "$OUTPUT_DIR"/tedge-mqtt-sub-retained-only.log 2>&1
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
