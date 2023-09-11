#!/bin/sh
set -e
tedge mqtt pub --qos 0 tedge/events/boot_event "$(printf '{"text": "Warning device is about to reboot!", "type": "device_boot"}')" 2>/dev/null
sleep 5
if command -v sudo >/dev/null 2>&1; then
    sudo shutdown -r now
else
    shutdown -r now
fi
