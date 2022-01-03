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
# C8YURL : e.g. https://thin-edge-io.eu-latest.cumulocity.com

# Adding sbin seems to be necessary for non Raspberry P OS systems as Debian or Ubuntu
PATH=$PATH:/usr/sbin

echo "Disconnect old bridge"

# Disconnect - may fail if not there
sudo tedge disconnect c8y

# From now on exit if a command exits with a non-zero status.
# Commands above are allowed to fail
set -e

cd $TEBASEDIR

# Check if clients are installed
dpkg -s mosquitto-clients

#sudo apt install librrd-dev python3-rrdtool rrdtool collectd

./ci/configure_bridge.sh

sudo cp ./configuration/contrib/collectd/collectd_analytics.conf /etc/collectd/collectd.conf
sudo cp ./configuration/contrib/collectd/collect_tedge.sh ~/
sudo chmod +x ~/collect_tedge.sh
sudo systemctl restart collectd

# Run all PySys tests

python3 -m venv ~/env-pysys
source ~/env-pysys/bin/activate

# use rrdtool here, for reasons we need a working c compliler, Python.h and others.
# We kind of like to avoid that for other systems
pip3 install -r tests/requirements_rrdtool.txt
cd tests/PySys/

set +e
pysys.py run --record -v DEBUG --include analytics
set -e

deactivate

mv __pysys_junit_xml pysys_junit_xml_analytics

cd $TEBASEDIR

sudo cp ./configuration/contrib/collectd/collectd.conf /etc/collectd/collectd.conf
sudo systemctl restart collectd

