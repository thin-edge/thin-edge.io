
echo "Disconnect old bridge"

# Disconnect - may fail if not there
sudo tedge disconnect c8y

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

sudo tedge config list

# Note: This will always upload a new certificate. From time to time
# we should delete the old ones in c8y
sudo -E tedge cert upload c8y --user $C8YUSERNAME

cat /etc/mosquitto/mosquitto.conf
