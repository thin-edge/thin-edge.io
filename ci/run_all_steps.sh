#!/usr/bin/sh

# Still clumsy integration of all ci steps: build, configure, smoke test
# Meant to be executed locally

# a simple checker function
appendtofile() {
    STRING=$1
    FILE=$2
    if grep "$STRING" $FILE; then
        echo 'line already there';
    else
        echo $STRING >> $FILE;
    fi
}

set -x

DEVICE=$1
USERNAME=$2
TENNANT=$3
C8YPASS=$4
DEVID=$5
TIMEZONE=$6

if [ -z $DEVICE ]; then
    echo "Error: Please supply your device name"
    exit 1
else
    echo "Your device: $DEVICE"
fi

if [ -z $USERNAME ]; then
    echo "Error: Please supply your user name"
    exit 1
else
    echo "Your user name: $USERNAME"
fi

if [ -z $TENNANT ]; then
    echo "Error: Please supply your tennant ID"
    exit 1
else
    echo "Your tennant ID: $TENNANT"
fi

if [ -z $C8YPASS ]; then
    echo "Error: Please supply your c8ypassword"
    exit 1
else
    echo "Your password: HIDDEN"
fi

if [ -z $TIMEZONE ]; then
    echo "Error: Please supply your timezone"
    exit 1
else
    echo "Your timezone: $TIMEZONE"
fi

echo "Preparing"

# adding sbin seems to be necessary for non raspbian debians
PATH=$PATH:/usr/sbin

# kill mapper
#sudo killall tedge_mapper

# Disconnect may fail if not there
sudo tedge disconnect c8y

set -e

#cd ~/

# Avoid deleting the tedge.toml as the device id is only set when
# the certificate is created.
#rm -rf .tedge

#cd ~/thin-edge.io

#git pull origin main

#cargo clean

rm -f ~/thin-edge.io/target/debian/*.deb

echo "Building"


cd ~/thin-edge.io

cargo deb -p tedge

cargo deb -p tedge_mapper

cargo build --example sawtooth_publisher
sudo dpkg -P mosquitto tedge tedge_mapper

echo "Installing"

sudo apt-get --assume-yes install mosquitto
#sudo apt-get install mosquitto libcjson1 libmosquitto1

ls -lah /etc/mosquitto/

sudo dpkg -i ~/thin-edge.io/target/debian/*.deb

echo "Configuring"

#mkdir ~/.tedge

#cp ~/certificate/tedge-certificate.pem ~/.tedge/
#cp ~/certificate/tedge-private-key.pem ~/.tedge/

sudo tedge cert show

sudo tedge config set c8y.url thin-edge-io.eu-latest.cumulocity.com

#tedge config set device.id $DEVICE

sudo tedge config set c8y.root.cert.path /etc/ssl/certs

#sudo chmod 777 /etc/mosquitto/mosquitto.conf

#FILE="/etc/mosquitto/mosquitto.conf"

#appendtofile "include_dir /home/$USER/.tedge/bridges" $FILE
#appendtofile "log_type debug" $FILE

#appendtofile "log_type error" $FILE
#appendtofile "log_type warning" $FILE
#appendtofile "log_type notice" $FILE
#appendtofile "log_type information" $FILE
#appendtofile "log_type subscribe" $FILE
#appendtofile "log_type unsubscribe" $FILE
#appendtofile "connection_messages true" $FILE

#sudo chmod 644 /etc/mosquitto/mosquitto.conf

cat /etc/mosquitto/mosquitto.conf
#cat ~/.tedge/tedge.toml

#chmod 666 ~/.tedge/*.pem

echo "Connect again"

sudo tedge connect c8y

# Start Mapper in the Background
#sudo tedge_mapper > ~/mapper.log 2>&1 &

cd ~/thin-edge.io

# Publish some values
tedge mqtt pub c8y/s/us 211,20
sleep 0.1
tedge mqtt pub c8y/s/us 211,30
sleep 0.1
tedge mqtt pub c8y/s/us 211,20
sleep 0.1
tedge mqtt pub c8y/s/us 211,30
sleep 0.1

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Uses SmartREST
./ci/roundtrip_local_to_c8y.py -m REST -pub ~/thin-edge.io/target/debug/examples/ -u $USERNAME -t $TENNANT -pass $C8YPASS -id $DEVID

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Uses thin-edge JSON
./ci/roundtrip_local_to_c8y.py -m JSON -pub ~/thin-edge.io/target/debug/examples/ -u $USERNAME -t $TENNANT -pass $C8YPASS -id $DEVID

