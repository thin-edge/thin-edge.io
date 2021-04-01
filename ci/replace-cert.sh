#!/usr/bin/sh

# To install expect:
# sudo apt-get install expect

# This script is intended to be executed by a GitHub self-hosted runner
# on a Raspberry Pi.
# TODO: Introduce certificate management

# Command line parameters:
# ci_smoke_test.sh  <timezone>
# Environment variables:
#    C8YDEVICE
#    C8YUSERNAME
#    C8YTENANT
#    C8YPASS
#    C8YDEVICEID

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
sudo killall tedge_mapper

# Disconnect - may fail if not there
sudo tedge disconnect c8y

# From now on exit if a command exits with a non-zero status.
# Commands above are allowed to fail
set -e

echo "Create new certificate"

sudo tedge cert remove

sudo tedge config set c8y.url thin-edge-io.eu-latest.cumulocity.com

DATE=$(date -u +"%Y-%m-%d_%H:%M")

sudo tedge cert create --device-id $C8YDEVICE-$DATE
#sudo tedge cert create --device-id octocatrpi3

# apt-get install expect
sudo expect -c "
spawn tedge cert upload c8y --user $C8YUSERNAME
expect \"Enter password:\"
send \"$C8YPASS\r\"
interact
"

sudo expect -c "
spawn sudo tedge cert upload c8y --user $C8YUSERNAME
expect \"Enter password:\"
send \"$C8YPASS\r\"
interact
"

expect -c "
spawn sudo tedge cert upload c8y --user $C8YUSERNAME
expect \"Enter password:\"
send \"$C8YPASS\r\"
interact
"

echo "Configuring Bridge"

sudo tedge cert show
sudo tedge config list
