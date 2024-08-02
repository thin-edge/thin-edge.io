#!/bin/sh
set -e

register_device() {
    DEVICE_ID=$(tedge config get device.id)
    C8Y_HOST=$(tedge config get c8y.url)
    export C8Y_HOST
    export CI=true
    RESP=

    echo "-----------------------------------------------------"
    echo "Device registration:    $DEVICE_ID"
    echo ""
    echo "Please register the above device id in Cumulocity IoT"
    echo "-----------------------------------------------------"
    echo "Waiting..."
    while :; do
        RESP=$(
            curl -sf -XPOST "https://$C8Y_HOST/devicecontrol/deviceCredentials" \
                --user "${C8Y_BOOTSTRAP_USER}:${C8Y_BOOTSTRAP_PASSWORD}" \
                -H "Content-Type: application/json" \
                -H "Accept: application/json" \
                -d "{\"id\":\"$DEVICE_ID\"}" || :
        )
        if [ -n "$RESP" ]; then
            HAS_TENANT=$(echo "$RESP" | jq -r 'has("tenantId") and has("username") and has("password")' 2>/dev/null ||:)
            if [ "$HAS_TENANT" = "true" ]; then
                break
            fi
        fi
        sleep 5
    done
    echo "Received device credentials"

    C8Y_DEVICE_USER=$(echo "$RESP" | jq -r '[.tenantId, .username] | join("/")')
    C8Y_DEVICE_PASSWORD=$(echo "$RESP" | jq -r '.password')

    # Save credentials
    PASSWORD_ESCAPED=$(echo "$C8Y_DEVICE_PASSWORD" | sed 's|\$|\\$|g')
    cat << EOT > /etc/tedge/c8y-mqtt.env
C8Y_DEVICE_USER="$C8Y_DEVICE_USER"
C8Y_DEVICE_PASSWORD="$PASSWORD_ESCAPED"
EOT
}

if [ ! -f /etc/tedge/c8y-mqtt.env ]; then
    register_device
else
    echo "Device is already registered"
fi

# shellcheck disable=SC1091
. /etc/tedge/c8y-mqtt.env

tedge config set c8y.username "$C8Y_DEVICE_USER"
tedge config set c8y.password "$C8Y_DEVICE_PASSWORD"
