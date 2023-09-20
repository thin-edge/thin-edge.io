#!/bin/bash
set -e

# Simulate the script being stopped via a signal
{
    sleep 2
    echo "Simulating SIGTERM interrupt"
    kill -SIGTERM $$
} &

# Configurable delay before rebooting
DELAY=30
if [ $# -gt 0 ] && [ "$1" -gt 0 ]; then
    DELAY="$1"
fi

echo "Sleeping $DELAY seconds"
sleep "$DELAY"
reboot
