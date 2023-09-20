#!/bin/sh
set -e

# Configurable delay before rebooting
DELAY=1
if [ $# -gt 0 ] && [ "$1" -gt 0 ]; then
    DELAY="$1"
fi

echo "Sleeping $DELAY seconds"
sleep "$DELAY"
