

echo "Tweak Mosquitto configuration"

sudo cp /usr/lib/systemd/system/mosquitto.service ci/mosquitto.service.bak

sudo cp ci/mosquitto.service /usr/lib/systemd/system/mosquitto.service

sudo systemctl daemon-reload
