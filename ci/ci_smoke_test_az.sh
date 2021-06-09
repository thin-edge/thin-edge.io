

set -e

# The bridge should be already configured
# lets avoid to create a new certifiate
# ./ci/configure_bridge.sh

THUMB=$(sudo tedge cert show | grep Thumb | cut -c13-)

echo "DEVICE Thumbprint is " $THUMB

./ci/az_upload_device_cert.py -d octocatrpi3 -t $THUMB -u ThinEdgeHub -s iothubowner

sudo tedge connect az

./ci/roundtrip_local_to_az.py -p sas_policy -b thinedgebus -q testqueue

sudo tedge disconnect az


