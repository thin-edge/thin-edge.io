#!/bin/sh
set -e

# This script serves as a template for collecting logs.
# To execute the script, use the following example command:
# ./00_template.sh collect --output-dir /tmp --config-dir /etc/tedge/diag-plugins

OUTPUT_DIR=""
CONFIG_DIR=${TEDGE_CONFIG_DIR:-/etc/tedge}
COMMAND=""

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --config-dir)
            # shellcheck disable=SC2034
            CONFIG_DIR="$2"
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
    echo "Output to stdout"
    echo "Output to stderr" >&2
    echo "Output to a file" > "$OUTPUT_DIR"/template.log 2>&1
}

# Execute the specified command
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
