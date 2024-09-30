#!/bin/sh

# Mocks a service monitored by tedge-watchdog
# Respond to health check command with a given response

set -e

SERVICE_NAME=${SERVICE_NAME:-"tedge-agent"}

HEALTH_CHECK_TOPIC="te/device/main/service/$SERVICE_NAME/cmd/health/check"
HEALTH_STATUS_TOPIC="te/device/main/service/$SERVICE_NAME/status/health"

# used to control whether this service responds to healthcheck requests or not
echo "Respond: $RESPOND"

while true; do
    mosquitto_sub -p 8883 --cafile /etc/mosquitto/ca_certificates/ca.crt -C 1 -t "$HEALTH_CHECK_TOPIC"
    echo "got check"
    if [ "$RESPOND" != 0 ]; then
        message=$(echo '{"status":"up"}' | jq -c --argjson pid $$ --argjson time "$(date +%s)" '. + {pid: $pid, time: $time}')
        mosquitto_pub -p 8883 --cafile /etc/mosquitto/ca_certificates/ca.crt -t "$HEALTH_STATUS_TOPIC" -m "$message"
        echo "sent response" "$message"
    fi
done
