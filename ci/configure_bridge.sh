#!/bin/bash

for variable in C8YURL C8YUSERNAME C8YPASS C8YDEVICE C8YTENANT IOTHUBNAME;
do
    declare -n var_ref=$variable
    if [ -z "${var_ref}" ]; then
        echo "$variable not set"
        exit 1
    fi
done

echo "Disconnect old bridge"

# Disconnect - may fail if not there
sudo tedge disconnect c8y

# From now on exit if a command exits with a non-zero status.
# Commands above are allowed to fail
set -e

echo "Configuring Bridge"

URL=$(echo "$C8YURL" | cut -c 9- - )

sudo tedge cert remove

# shellcheck disable=SC2153
sudo tedge cert create --device-id="$C8YDEVICE"

sudo tedge cert show

sudo tedge config set c8y.url "$URL"

sudo tedge config set c8y.root_cert_path /etc/ssl/certs

sudo tedge config set az.url "$IOTHUBNAME.azure-devices.net"

sudo tedge config set az.root_cert_path /etc/ssl/certs/Baltimore_CyberTrust_Root.pem

sudo tedge config list

# Note: This will always upload a new certificate. From time to time
# we should delete the old ones in c8y
sudo -E tedge cert upload c8y --user "$C8YUSERNAME"

cat /etc/mosquitto/mosquitto.conf

python3 -m venv ~/env-c8y-api
# shellcheck disable=SC1090
source ~/env-c8y-api/bin/activate
pip3 install c8y-api retry-decorator

# Delete the device (ignore error)
set +e
python3 ./ci/delete_current_device_c8y.py --tenant "$C8YTENANT" --user "$C8YUSERNAME" --device "$C8YDEVICE" --url "$C8YURL"
set -e

# Give Cumolocity time to process the cert deletion
sleep 2

# Connect and disconnect so that we can retrieve a new device ID
sudo tedge connect c8y
sudo tedge disconnect c8y

# Give Cumolocity time to process the cert deletion
sleep 2

# Retrieve the Cumulocity device ID

C8YDEVICEID=$(python3 ./ci/find_device_id.py --tenant "$C8YTENANT" --user "$C8YUSERNAME" --device "$C8YDEVICE" --url "$C8YURL")
export C8YDEVICEID

echo "The new device ID is: " "$C8YDEVICEID"

deactivate

