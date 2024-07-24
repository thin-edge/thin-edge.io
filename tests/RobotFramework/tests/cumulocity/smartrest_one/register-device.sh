#!/bin/sh
set -e

install_dependencies() {
    if ! command -V c8y >/dev/null 2>&1; then
        curl https://reubenmiller.github.io/go-c8y-cli-repo/debian/PUBLIC.KEY | gpg --dearmor | sudo tee /usr/share/keyrings/go-c8y-cli-archive-keyring.gpg >/dev/null
        sudo sh -c "echo 'deb [signed-by=/usr/share/keyrings/go-c8y-cli-archive-keyring.gpg] http://reubenmiller.github.io/go-c8y-cli-repo/debian stable main' >> /etc/apt/sources.list"
        sudo apt-get update
        sudo apt-get install -y --no-install-recommends go-c8y-cli
    fi
}

register_device() {
    DEVICE_ID=$(tedge config get device.id)
    C8Y_HOST=$(tedge config get c8y.url)
    export C8Y_HOST
    export CI=true

    while :; do
        echo "Device registration loop:    $DEVICE_ID"
        CREDS=$(c8y deviceregistration getCredentials --id "$DEVICE_ID" --sessionUsername "$C8Y_BOOTSTRAP_USER" --sessionPassword "$C8Y_BOOTSTRAP_PASSWORD" --select tenantid,username,password -o csv ||:)
        if [ -n "$CREDS" ]; then
            break
        fi
        sleep 5
    done

    DEVICE_TENANT=$(echo "$CREDS" | cut -d, -f1)
    DEVICE_USERNAME=$(echo "$CREDS" | cut -d, -f2)
    DEVICE_PASSWORD=$(echo "$CREDS" | cut -d, -f3)

    # Save credentials
    cat << EOT > /etc/tedge/c8y-mqtt.env
DEVICE_TENANT="$DEVICE_TENANT"
DEVICE_USERNAME="$DEVICE_USERNAME"
DEVICE_PASSWORD="$DEVICE_PASSWORD"
EOT

    # Show banner
    # echo
    # echo "--------------- device credentials --------------"
    # echo "DEVICE_TENANT:      $DEVICE_TENANT"
    # echo "DEVICE_USERNAME:    $DEVICE_USERNAME"
    # echo "DEVICE_PASSWORD:    $DEVICE_PASSWORD"
    # echo "-------------------------------------------------"
}

install_dependencies

if [ ! -f /etc/tedge/c8y-mqtt.env ]; then
    register_device
fi

# shellcheck disable=SC1091
. /etc/tedge/c8y-mqtt.env

if [ -f /etc/tedge/mosquitto-conf/c8y-bridge.conf ]; then
    echo "Updating c8y bridge username/password"
    if ! grep -q remote_username /etc/tedge/mosquitto-conf/c8y-bridge.conf; then
        sed -i 's|bridge_certfile .*|remote_username '"$DEVICE_TENANT/$DEVICE_USERNAME"'|' /etc/tedge/mosquitto-conf/c8y-bridge.conf
        sed -i 's|bridge_keyfile .*|remote_password '"$DEVICE_PASSWORD"'|' /etc/tedge/mosquitto-conf/c8y-bridge.conf
    else
        sed -i 's|remote_username .*|remote_username '"$DEVICE_TENANT/$DEVICE_USERNAME"'|' /etc/tedge/mosquitto-conf/c8y-bridge.conf
        sed -i 's|remote_password .*|remote_password '"$DEVICE_PASSWORD"'|' /etc/tedge/mosquitto-conf/c8y-bridge.conf
    fi

    # TODO delete JWT topics
    # topic s/uat out 0 c8y/ ""
    # topic s/dat in 0 c8y/ ""
fi
