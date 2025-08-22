#!/bin/sh
set -e

OUTPUT_DIR=""
COMMAND=""
TEDGE_CONFIG_DIR=${TEDGE_CONFIG_DIR:-/etc/tedge}

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

mosquitto_journal() {
    journalctl -u "mosquitto" -n 1000 --no-pager > "$OUTPUT_DIR/mosquitto-journal.log" 2>&1 ||:
}

mosquitto_log() {
    if [ -f /var/log/mosquitto/mosquitto.log ]; then
        cp /var/log/mosquitto/mosquitto.log "$OUTPUT_DIR"/
    else
        echo "mosquitto.log not found" >&2
    fi
}

mosquitto_config() {
    if command -V tree >/dev/null >&2; then
        tree /etc/mosquitto > "$OUTPUT_DIR/etc_mosquitto.tree.txt" ||:
    fi

    mkdir -p "$OUTPUT_DIR/mosquitto"
    if [ -f /etc/mosquitto/mosquitto.conf ]; then
        cp -aR /etc/mosquitto/mosquitto.conf "$OUTPUT_DIR/mosquitto" ||:
    fi
    if [ -d /etc/mosquitto/conf.d ]; then
        cp -aR /etc/mosquitto/conf.d "$OUTPUT_DIR/mosquitto/" ||:
    fi

    mkdir -p "$OUTPUT_DIR/tedge"
    cp -aR "$TEDGE_CONFIG_DIR/mosquitto-conf" "$OUTPUT_DIR/tedge/" ||:

    # sanitize password fields
    find "$OUTPUT_DIR" -name "*.conf" -exec sed -i 's/password\s*.*/password <redacted>/g' {} \; ||:
}

collect() {
    if command -V mosquitto > /dev/null 2>&1; then
        if command -V journalctl >/dev/null 2>&1; then
            mosquitto_journal
        fi
        mosquitto_log
        mosquitto_config
    else
        echo "mosquitto not found" >&2
        # this plugin is not applicable when mosquitto doesn't exist
        exit 2
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
