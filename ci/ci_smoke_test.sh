#!/usr/bin/sh

# This script is intended to be executed by a GitHub self-hosted runner
# on a Raspberry Pi.
# TODO: Introduce certificate management

# Smoke test
# - Rebuild bridge
# - Publish some values with tedge cli
# - Run a smoke test for c8y smartREST
# - Run a smoke test for c8y Thin Edge JSON

# Command line parameters:
# ci_smoke_test.sh  <timezone>
# Environment variables:
#    C8YDEVICE
#    C8YUSERNAME
#    C8YTENANT
#    C8YPASS
#    C8YDEVICEID

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

if [ -z $C8YDEVICE ]; then
    echo "Error: Please supply your device name as environment variable C8YDEVICE"
    exit 1
else
    echo "Your device: HIDDEN"
fi

if [ -z $C8YDEVICEID ]; then
    echo "Error: Please supply your Cumulocity device ID  name as environment variable C8YDEVICEID"
    exit 1
else
    echo "Your device: HIDDEN"
fi


if [ -z $C8YUSERNAME ]; then
    echo "Error: Please supply your user name  as environment variable C8YUSERNAME"
    exit 1
else
    echo "Your user name: HIDDEN"
fi

if [ -z $C8YTENANT ]; then
    echo "Error: Please supply your tenant ID as environment variable C8YTENANT"
    exit 1
else
    echo "Your tenant ID: HIDDEN"
fi

if [ -z $C8YPASS ]; then
    echo "Error: Please supply your Cumulocity password environment variable C8YPASS"
    exit 1
else
    echo "Your password: HIDDEN"
fi

# Adding sbin seems to be necessary for non Raspberry P OS systems as Debian or Ubuntu
PATH=$PATH:/usr/sbin

echo "Disconnect old bridge"

# Disconnect - may fail if not there
sudo tedge disconnect c8y

# From now on exit if a command exits with a non-zero status.
# Commands above are allowed to fail
set -e

echo "Configuring Bridge"

sudo tedge cert remove

sudo tedge cert create --device-id=$C8YDEVICE

sudo tedge cert show

sudo -E tedge cert upload c8y --user $C8YUSERNAME

sudo tedge config list

sudo tedge config set c8y.url thin-edge-io.eu-latest.cumulocity.com

cat /etc/mosquitto/mosquitto.conf

echo "Connect again"
sudo tedge connect c8y

echo "Start smoke tests"

# Publish some values
for val in 20 30 20 30; do
    tedge mqtt pub c8y/s/us 211,$val
    sleep 0.1
done

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Uses SmartREST for publishing
./ci/roundtrip_local_to_c8y.py -m REST -pub ./examples/ -u $C8YUSERNAME -t $C8YTENANT -pass $C8YPASS -id $C8YDEVICEID

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Set executable bit as it was just downloaded
chmod +x ./examples/sawtooth_publisher

# Make a backup so that we can use it later, github will clean up after running
# TODO: Find a better solution for binary management
cp ./examples/sawtooth_publisher ~/

# Uses thin-edge JSON for publishing
./ci/roundtrip_local_to_c8y.py -m JSON -pub ./examples/ -u $C8YUSERNAME -t $C8YTENANT -pass $C8YPASS -id $C8YDEVICEID

