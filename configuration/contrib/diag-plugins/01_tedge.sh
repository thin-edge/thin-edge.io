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

# Collect logs for a given service
collect_logs() {
    SERVICE="$1"
    if command -V journalctl >/dev/null 2>&1; then
        journalctl -u "$SERVICE" -n 1000 --no-pager > "$OUTPUT_DIR/${SERVICE}.log" 2>&1 ||:
    fi
}

collect() {
    # tedge-agent
    collect_logs "tedge-agent"

    # Collect logs for each mapper
    CLOUDS="c8y az aws"
    for cloud in $CLOUDS; do
        if tedge config get "${cloud}.url" >/dev/null 2>&1; then
            collect_logs "tedge-mapper-${cloud}"
        fi
    done

    # Collectd mapper log
    if systemctl list-unit-files tedge-mapper-collectd.service  >/dev/null 2>&1; then
        collect_logs tedge-mapper-collectd
    fi

    # Copy tedge.toml
    cp "$TEDGE_CONFIG_DIR"/tedge.toml "$OUTPUT_DIR"/tedge.toml

    # tedge config list
    tedge config list --all > "$OUTPUT_DIR"/tedge-config-list.log
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
