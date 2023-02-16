#!/bin/sh
set -e

# Wait for publish to be successful as the mosquitto client can take a while to start
while true
do
    if ! tedge mqtt pub --retain --qos 0 tedge/events/boot_event "$(printf '{"text": "device booted up 🟢 %s", "type": "device_boot"}' "$(uname -a)")" 2>/dev/null
    then
        sleep 1
    else
        break
    fi
done
