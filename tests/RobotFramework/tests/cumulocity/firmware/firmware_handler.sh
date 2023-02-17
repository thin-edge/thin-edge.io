#!/bin/bash

info() {
    echo "$(date --iso-8601=seconds 2>/dev/null || date -Iseconds) : INFO : $*" >&2
}

# Parse message
TEMPLATE_ID=$(echo "$1" | cut -d, -f1)
DEVICE_SN=$(echo "$1" | cut -d, -f2)
NAME=$(echo "$1" | cut -d, -f3)
VERSION=$(echo "$1" | cut -d, -f4)
URL=$(echo "$1" | cut -d, -f5)

echo "Installing firmware"
echo "TEMPLATE_ID: $TEMPLATE_ID"
echo "DEVICE_SN: $DEVICE_SN"
echo "NAME: $NAME"
echo "NAME: $NAME"
echo "VERSION: $VERSION"
echo "URL: $URL"

if [ -f "$URL" ]; then
    echo "URL is actually a local file path"
else
    echo "URL is a URL which needs to be downloaded"
fi

# Add simple error handling (to assist in unhappy path testing)
if [ -z "$NAME" ]; then
    info "Invalid firmware name. Firmware name cannot be empty"
    exit 1
fi

case "$NAME" in
    *)
        echo "Installing firmware using default handler"
        sleep 2
        tedge mqtt pub -q 1 c8y/s/us "115,${NAME},${VERSION},${URL}"
        ;;
esac

EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
    info "Firmware returned a non-zero exit code. code=$EXIT_CODE"
fi
exit $EXIT_CODE
