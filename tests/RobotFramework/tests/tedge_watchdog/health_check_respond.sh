#!/bin/sh

# Respond to health check command with a given response

set -e

# # e.g. tedge-agent
# SERVICE_NAME=$1

# # e.g. '{"status":"up", "pid":2137, "time":1727439518}'
# RESPONSE=$2

SERVICE_NAME=${SERVICE_NAME:-"tedge-agent"}
RESPONSE=${RESPONSE:-'{"status":"up"}'}

HEALTH_CHECK_TOPIC="te/device/main/service/$SERVICE_NAME/cmd/health/check"
HEALTH_STATUS_TOPIC="te/device/main/service/$SERVICE_NAME/status/health"

message=$(echo "$RESPONSE" | jq -c --argjson pid $$ --argjson time $(date +%s) '. + {pid: $pid, time: $time}')

while true; do
    mosquitto_sub -p 8883 --cafile /etc/mosquitto/ca_certificates/ca.crt -C 1 -t $HEALTH_CHECK_TOPIC
    echo "got check"
    mosquitto_pub -p 8883 --cafile /etc/mosquitto/ca_certificates/ca.crt -t $HEALTH_STATUS_TOPIC -m "$message"
    echo "sent response" $message
done
