#!/usr/bin/bash

# Smoke test for Azure IoT
# The bridge should be already configured (done by configure_bridge.sh)
# lets avoid to create a new certifiate in this script as it is shared with C8y.

# This script is intended to be executed by a GitHub self-hosted runner
# on a Raspberry Pi.

# Disconnect - may fail if not there
sudo tedge disconnect az
sudo tedge disconnect c8y

set -e

# The bridge should be already configured
# lets avoid to create a new certifiate here ()
# ./ci/configure_bridge.sh

#set temporarily here
sudo tedge config set az.url ThinEdgeCiHub.azure-devices.net

# Read device thumbprint from command line
THUMB=$(sudo tedge cert show | grep Thumb | cut -c13-)
echo "DEVICE Thumbprint is " $THUMB


python3 -m venv ~/env-eventhub
source ~/env-eventhub/bin/activate
pip install azure-eventhub

./ci/az_upload_device_cert.py -d octocatrpi3 -t $THUMB -u ThinEdgeCiHub -s iothubowner

sudo tedge connect az

# Get messages from a service bus
#./ci/roundtrip_local_to_az.py -p sas_policy2 -b thinedgebus -q testqueue2
# Use Azure SDK to access the IoT Hub
./ci/roundtrip_local_to_az.py eventhub

sudo tedge disconnect az

deactivate

