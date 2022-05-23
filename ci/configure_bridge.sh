#!/bin/bash

echo "Disconnect old bridge"

# Disconnect - may fail if not there
sudo tedge disconnect c8y
suod tedge disconnect az

# From now on exit if a command exits with a non-zero status.
# Commands above are allowed to fail
set -e

echo "Configuring Bridge"

URL=$(echo $C8YURL | cut -c 9- - )

sudo tedge cert remove

sudo tedge cert create --device-id=$C8YDEVICE

sudo tedge cert show

sudo tedge config set c8y.url $URL

sudo tedge config set c8y.root.cert.path /etc/ssl/certs

sudo tedge config set az.url $IOTHUBNAME.azure-devices.net

sudo tedge config set az.root.cert.path /etc/ssl/certs/Baltimore_CyberTrust_Root.pem

# Enable for Azure (see also ci_smoke_test_az.sh)
#
# THUMB=$(sudo tedge cert show | grep Thumb | cut -c13-)
# echo "DEVICE Thumbprint is " $THUMB
# python3 ci/az_upload_device_cert.py -d $C8YDEVICE -t $THUMB -u $IOTHUBNAME -s iothubowner

sudo tedge config list

# Note: This will always upload a new certificate. From time to time
# we should delete the old ones in c8y
sudo -E tedge cert upload c8y --user $C8YUSERNAME

cat /etc/mosquitto/mosquitto.conf

python3 -m venv ~/env-c8y-api
source ~/env-c8y-api/bin/activate
pip3 install c8y-api retry-decorator

# Delete the device (ignore error)
set +e
python3 ./ci/delete_current_device_c8y.py --tenant $C8YTENANT --user $C8YUSERNAME --device $C8YDEVICE --url $C8YURL
set -e

# Give Cumolocity time to process the cert deletion
sleep 2

# Connect and disconnect so that we can retrive a new device ID
sudo tedge connect c8y

# Wait for some time so that we can upload a software list
sleep 2

sudo tedge disconnect c8y

# Give Cumolocity time to process the new certificate
sleep 2

# Retrieve the Cumulocity device ID

export C8YDEVICEID=$(python3 ./ci/find_device_id.py --tenant $C8YTENANT --user $C8YUSERNAME --device $C8YDEVICE --url $C8YURL)

echo "The new device ID is: " $C8YDEVICEID

deactivate

