

echo "Tweak Mosquitto configuration"

# Rpi at Mythic Beasts
if [ -f  /lib/systemd/system/mosquitto.service ]; then
    echo "This is a hosted Pi"
    SERVICEFILE=/lib/systemd/system/mosquitto.service
fi

# Regular Rpi and Debian
if [ -f  /usr/lib/systemd/system/mosquitto.service ]; then
    echo "This is a regular Debian"
    SERVICEFILE=/usr/lib/systemd/system/mosquitto.service
fi

sudo rm -f ci/mosquitto.service.bak
sudo cp $SERVICEFILE ci/mosquitto.service.bak
sudo cp ci/mosquitto.service $SERVICEFILE


sudo systemctl daemon-reload
