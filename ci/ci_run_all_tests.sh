#!/usr/bin/bash

# Run all available system-tests.
# Note: Needs a bash shell to run
#
# Expected environment variables to be set:
# C8YPASS : Cumulocity password
# C8YUSERNAME : Cumolocity username
# C8YTENANT : Cumolocity tenant
# C8YDEVICE : The device name
# C8YDEVICEID : The device ID in Cumolocity
# TIMEZONE : Your timezone (temporary)
# TEBASEDIR : Base directory for the Thin-Edge repo
# EXAMPLEDIR : The direcory of the sawtooth example

# Adding sbin seems to be necessary for non Raspberry P OS systems as Debian or Ubuntu
PATH=$PATH:/usr/sbin

echo "Disconnect old bridge"

# Disconnect - may fail if not there
sudo tedge disconnect c8y

# From now on exit if a command exits with a non-zero status.
# Commands above are allowed to fail
set -e

cd $TEBASEDIR

# Check if clients are installed. If not, run:
# sudo apt-get install mosquitto-clients

dpkg -s mosquitto-clients

#sudo apt install librrd-dev python3-rrdtool rrdtool collectd

./ci/configure_bridge.sh

# Run all PySys tests

python3 -m venv ~/env-pysys
source ~/env-pysys/bin/activate
pip3 install -r tests/requirements.txt
cd tests/PySys/

#pysys.py run -v DEBUG
pysys.py run -v DEBUG 'publish_sawmill_record_statisti*'


# Run the tests for the apt plugin
# These are skipped by default, as the modify the operating system
# TODO Ideally, as the keyword says we should run them in a container
# pysys.py run 'apt_*' -XmyPlatform='container'

# Disabled them until the agent is integrated right
#
## Run the tests for software management
# pysys.py run 'sm-apt-*' -XmyPlatform='specialcontainer'

deactivate
