
set -e

echo "Configuring Bridge"

# Squelch the https:// from the url
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

./ci/find_device_id.py --tenant $C8YTENANT --user $C8YUSERNAME --device $C8YDEVICE --url $URL > ~/C8YDEVICEID

# Later: export C8YDEVICEID=$(cat ~/C8YDEVICEID)
C8YDEVICEID=$(cat ~/C8YDEVICEID)
echo "The current device ID is (read from home directory): " $C8YDEVICEID

