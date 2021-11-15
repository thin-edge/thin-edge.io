#!/usr/bin/sh

# Smoke test for Azure IoT
# The bridge should be already configured (done by configure_bridge)
# lets avoid to create a new certifiate

# - Rebuild bridge
# - Run a roundtrip test for Azure

# This script is intended to be executed by a GitHub self-hosted runner
# on a Raspberry Pi.

# Disconnect - may fail if not there
sudo tedge disconnect az

sudo tedge disconnect c8y

set -e

# The bridge should be already configured
# lets avoid to create a new certifiate
# ./ci/configure_bridge.sh

# Read device thumbprint from command line
THUMB=$(sudo tedge cert show | grep Thumb | cut -c13-)
echo "DEVICE Thumbprint is " $THUMB


python3 -m venv ~/env-eventhub
source ~/env-eventhub/bin/activate
pip install azure-eventhub

./ci/az_upload_device_cert.py -d octocatrpi3 -t $THUMB -u ThinEdgeHub -s iothubowner

sudo tedge connect az

./ci/roundtrip_local_to_az.py -p sas_policy2 -b thinedgebus -q testqueue2

sudo tedge disconnect az

deactivate

