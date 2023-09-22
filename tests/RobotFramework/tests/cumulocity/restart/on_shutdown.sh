#!/bin/sh
set -e
TARGET="$(tedge config get mqtt.topic_root)/$(tedge config get mqtt.device_topic_id)"
tedge mqtt pub --qos 0 "$TARGET/e/device_boot" '{"text": "Warning device is about to reboot!"}' 2>/dev/null

# Configurable delay before rebooting
DELAY=1
if [ $# -gt 0 ] && [ "$1" -gt 0 ]; then
    DELAY="$1"
fi

echo "Sleeping $DELAY seconds"
sleep "$DELAY"

use_sudo() {
    command -v sudo >/dev/null 2>&1 && [ "$(id -u)" != "0" ]
}

# Note: Delaying the shutdown using 'shutdown -r +1' does not work when using systemd inside a container
REBOOT="shutdown -r now"

if use_sudo; then
    # shellcheck disable=SC2086
    sudo $REBOOT
else
    $REBOOT
fi
