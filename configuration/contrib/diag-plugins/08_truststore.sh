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
    # Directory listing
    if [ -d /etc/ssl/certs ]; then
        # list certs and show symlinks
        ls -l /etc/ssl/certs > "$OUTPUT_DIR/etc_ssl_certs.txt" ||:
    else
        echo "Directory /etc/ssl/certs does not exist" >&2
    fi

    if [ -f /etc/ssl/certs/ca-certificates.crt ]; then
        echo "Copying /etc/ssl/certs/ca-certificates.crt" >&2
        cp -a /etc/ssl/certs/ca-certificates.crt "$OUTPUT_DIR/"
    else
        echo "File /etc/ssl/certs/ca-certificates.crt does not exist" >&2
    fi

    # Check for ca-certificates package
    if command -V dpkg >/dev/null 2>&1; then
        echo "dpkg ca-certificates package" >&2
        dpkg --list | grep ca-certificates >&2 ||:
    fi
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
