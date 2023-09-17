#!/bin/sh
set -e
TARGET="$(tedge config get mqtt.topic_root)/$(tedge config get mqtt.device_topic_id)"
tedge mqtt pub --qos 0 "$TARGET/e/device_boot" "$(printf '{"text": "Warning device is about to reboot!"}')" 2>/dev/null
sleep 5
if command -v sudo >/dev/null 2>&1; then
    sudo shutdown -r now
else
    shutdown -r now
fi
