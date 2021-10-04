#!/usr/bin/sh

# Smoke test for Cumulocity
# - Rebuild bridge
# - Publish some values with tedge cli
# - Run a smoke test for c8y smartREST
# - Run a smoke test for c8y Thin Edge JSON

# This script is intended to be executed by a GitHub self-hosted runner
# on a Raspberry Pi.

# Command line parameters:
# ci_smoke_test.sh  <timezone>
# Environment variables:
#    C8YDEVICE
#    C8YUSERNAME
#    C8YTENANT
#    C8YPASS
#    C8YDEVICEID
#    EXAMPLEDIR

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

if [ -z $EXAMPLEDIR ]; then
    echo "Error: Please supply the path to the sawtooth_publisher as EXAMPLEDIR"
    exit 1
else
    echo "Your password: EXAMPLEDIR"
fi

TEBASEDIR


# Adding sbin seems to be necessary for non Raspberry P OS systems as Debian or Ubuntu
PATH=$PATH:/usr/sbin

echo "Disconnect old bridge"

# Disconnect - may fail if not there
sudo tedge disconnect c8y

# From now on exit if a command exits with a non-zero status.
# Commands above are allowed to fail
set -e

./ci/configure_bridge.sh

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
./ci/roundtrip_local_to_c8y.py -m REST -pub $EXAMPLEDIR -u $C8YUSERNAME -t $C8YTENANT -id $C8YDEVICEID

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Uses thin-edge JSON for publishing
./ci/roundtrip_local_to_c8y.py -m JSON -pub $EXAMPLEDIR -u $C8YUSERNAME -t $C8YTENANT -id $C8YDEVICEID

echo "Disonnect again"
sudo tedge disconnect c8y
