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

entity_store() {
    STATE_DIR=$(tedge config get agent.state.path)
    if [ -f "$STATE_DIR"/entity_store.jsonl ]; then
        cp "$STATE_DIR"/entity_store.jsonl "$OUTPUT_DIR"/
    elif [ -f "$TEDGE_CONFIG_DIR"/.agent/entity_store.jsonl ]; then
        cp "$TEDGE_CONFIG_DIR"/.agent/entity_store.jsonl "$OUTPUT_DIR"/
    elif [ -f "$TEDGE_CONFIG_DIR"/.tedge-mapper-c8y/entity_store.jsonl ]; then
        cp "$TEDGE_CONFIG_DIR"/.tedge-mapper-c8y/entity_store.jsonl  "$OUTPUT_DIR"/
    else
        echo "entity_store.jsonl not found in $STATE_DIR, $TEDGE_CONFIG_DIR/.agent or $TEDGE_CONFIG_DIR/.tedge-mapper-c8y" >&2
    fi
}

collect() {
    entity_store
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
