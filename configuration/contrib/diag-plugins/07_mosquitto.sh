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

mosquitto_journal() {
    if command -V journalctl >/dev/null 2>&1; then
        journalctl -u "mosquitto" -n 1000 --no-pager > "$OUTPUT_DIR/mosquitto-journal.log" 2>&1 ||:
    fi
}

mosquitto_log() {
    if [ -f /var/log/mosquitto/mosquitto.log ]; then 
        cp /var/log/mosquitto/mosquitto.log "$OUTPUT_DIR"/
    else
        echo "mosquitto.log not found" >&2
    fi
}

collect() {
    if [ "$(tedge config get mqtt.bridge.built_in)" = "false" ]; then
        if command -V mosquitto > /dev/null 2>&1; then
            mosquitto_journal
            mosquitto_log
        else
            echo "mosquitto not found" >&2
        fi
    else
      # built-in bridge is used, hence this plugin should be skipped
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
