#!/bin/sh
set -e

TARGET="$(tedge config get mqtt.topic_root)/$(tedge config get mqtt.device_topic_id)"

# Wait for publish to be successful as the mosquitto client can take a while to start
while true
do
    if ! tedge mqtt pub --retain --qos 0 "$TARGET/e/device_boot" "$(printf '{"text": "device booted up 🟢 %s"}' "$(uname -a)")" 2>/dev/null
    then
        sleep 1
    else
        break
    fi
done
