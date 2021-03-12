#!/usr/bin/sh

# This script is intended to be executed by a GitHub self-hosted runner
# on a Raspberry Pi.
# TODO: Fix certificate management

# Smoke test
# - Rebuild bridge
# - Run a test for c8y smartREST
# - Run a test for c8y Thin Edge JSON

# Command line parameters:
# ci_smoke_test.sh <device> <username> <tennantid> <password> <deviceid> <timezone>

# a simple checker function
check() {
    if [ $? -ne 0 ]; then
        echo "Error: Exiting due to previous error"
        exit 1;
    fi
}

# a simple function to append lines to files if not already there
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

# Adding sbin seems to be necessary for non raspbian debians
PATH=$PATH:/usr/sbin

# We start from the thin-edge base directory
# This should work locally and as well in GitHub workflows on new checkouts
STARTPATH=$(pwd)

echo "Disconnect old bridge"

# Kill mapper - may fail if not running
killall tedge_mapper
#check

# Disconnect - may fail if not there
tedge disconnect c8y
#check

# We skipp Certificate handling for now. The certificates and the tedge.toml
# need to be already there!
# TODO Add proper certificate handling, create new one or reuse
#
#rm -rf ~/.tedge
#check
#
#mkdir ~/.tedge
#check
#
#echo "Recreating configuration"
#
#echo "[device]
#id = 'octocat_device'
#key_path = '/home/pi/.tedge/tedge-private-key.pem'
#cert_path = '/home/pi/.tedge/tedge-certificate.pem'
#
#[c8y]
#
#[azure]
#" > ~/.tedge/tedge.toml
#
#ls -lah ~/.tedge/
#
#echo $PRIVATE_KEY_8CAT
#
##if [ -z $PRIVATE_KEY_8CAT ]; then
##    echo "Error: PRIVATE_KEY_8CAT not set"
##    exit 1;
##fi
#
#echo $PRIVATE_KEY_8CAT > ~/.tedge/tedge-private-key.pem
#check
#
#echo $PUBLIC_KEY_8CAT
#
##if [ -z $PUBLIC_KEY_8CAT ]; then
##    echo "Error: PUBLIC_KEY_8CAT not set"
##    exit 1;
##fi
#
#echo $PUBLIC_KEY_8CAT > ~/.tedge/tedge-certificate.pem
#check

echo "Configuring Bridge"

tedge cert show
check

cp configuration/broker/configuration/cumulocity/c8y-trusted-root-certificates.pem ~/.tedge/
check

ls -lah ~/.tedge/

tedge config set c8y.url mqtt.latest.stage.c8y.io:8883
check

# TODO Fix setting the device.id this depends on current development
#tedge config set device.id $DEVICE
#check

tedge config set c8y.root.cert.path ~/.tedge/c8y-trusted-root-certificates.pem
check


# Set the access rights, so that we can access the file
sudo chmod 777 /etc/mosquitto/mosquitto.conf
check

FILE="/etc/mosquitto/mosquitto.conf"

appendtofile "include_dir /home/$USER/.tedge/bridges" $FILE
appendtofile "log_type debug" $FILE
appendtofile "log_type error" $FILE
appendtofile "log_type warning" $FILE
appendtofile "log_type notice" $FILE
appendtofile "log_type information" $FILE
appendtofile "log_type subscribe" $FILE
appendtofile "log_type unsubscribe" $FILE
appendtofile "connection_messages true" $FILE

# Set proper access right again
sudo chmod 644 /etc/mosquitto/mosquitto.conf
check

cat /etc/mosquitto/mosquitto.conf
check

cat ~/.tedge/tedge.toml
check

chmod 666 ~/.tedge/c8y-trusted-root-certificates.pem
check

chmod 666 ~/.tedge/*.pem
check

echo "Connect again"
tedge -v connect c8y
check

#Start Mapper in the Background
tedge_mapper > ~/mapper.log 2>&1 &

# Go back to the path where we started
cd $STARTPATH
check

echo "Start smoke tests"

# Publish some values
tedge mqtt pub c8y/s/us 211,20
check
sleep 0.1
check
tedge mqtt pub c8y/s/us 211,30
check
sleep 0.1
check
tedge mqtt pub c8y/s/us 211,20
check
sleep 0.1
check
tedge mqtt pub c8y/s/us 211,30
check
sleep 0.1
check

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Uses SmartREST
./ci/roundtrip_local_to_c8y.py -m REST -pub ./examples/ -u $USERNAME -t $TENNANT -pass $C8YPASS -id $DEVID -z $TIMEZONE
check

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Set executable bit as it was just downloaded
chmod +x ./examples/sawtooth_publisher
check

# Make a backup so that we can use it later
cp ./examples/sawtooth_publisher ~/

# Uses thin-edge JSON
./ci/roundtrip_local_to_c8y.py -m JSON -pub ./examples/ -u $USERNAME -t $TENNANT -pass $C8YPASS -id $DEVID -z $TIMEZONE
check

