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

TIMEZONE=$1

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

if [ -z $TIMEZONE ]; then
    echo "Error: Please supply your timezone"
    exit 1
else
    echo "Your timezone: $TIMEZONE"
fi

# Adding sbin seems to be necessary for non Raspberry P OS systems as Debian or Ubuntu
PATH=$PATH:/usr/sbin

echo "Disconnect old bridge"

# Kill mapper - may fail if not running
killall tedge_mapper

# Disconnect - may fail if not there
tedge disconnect c8y

# From now on exit if a command exits with a non-zero status.
# Commands above are allowed to fail
set -e

echo "Configuring Bridge"

tedge cert show

ls -lah ~/.tedge/

tedge config set c8y.url octocat.eu-latest.cumulocity.com

tedge config set c8y.root.cert.path /etc/ssl/certs/Go_Daddy_Class_2_CA.pem

# Store permissions for later
ATTR=$(stat -c "%a" /etc/mosquitto/mosquitto.conf)

# Set r/w permissions, so that we can access the file
sudo chmod 666 /etc/mosquitto/mosquitto.conf

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
sudo chmod $ATTR /etc/mosquitto/mosquitto.conf

cat /etc/mosquitto/mosquitto.conf

cat ~/.tedge/tedge.toml

chmod 666 ~/.tedge/c8y-trusted-root-certificates.pem

chmod 666 ~/.tedge/*.pem

echo "Connect again"
tedge -v connect c8y

#Start Mapper in the Background
tedge_mapper > ~/mapper.log 2>&1 &

echo "Start smoke tests"

# Publish some values
for val in 20 30 20 30; do
    tedge mqtt pub c8y/s/us 211,$val
    sleep 0.1
done

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Uses SmartREST for publishing
./ci/roundtrip_local_to_c8y.py -m REST -pub ./examples/ -u $C8YUSERNAME -t $C8YTENANT -pass $C8YPASS -id $C8YDEVICEID -z $TIMEZONE

# Wait some seconds until our 10 seconds window is empty again
sleep 12

# Set executable bit as it was just downloaded
chmod +x ./examples/sawtooth_publisher

# Make a backup so that we can use it later, github will clean up after running
# TODO: Find a better solution for binary management
cp ./examples/sawtooth_publisher ~/

# Uses thin-edge JSON for publishing
./ci/roundtrip_local_to_c8y.py -m JSON -pub ./examples/ -u $C8YUSERNAME -t $C8YTENANT -pass $C8YPASS -id $C8YDEVICEID -z $TIMEZONE

