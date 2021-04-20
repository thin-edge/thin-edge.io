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

set -e

cd $TEBASEDIR

# Check if clients are installed
dpkg -s mosquitto-clients

# Run all PySys tests

python3 -mvenv ~/env-pysys

source ~/env-pysys/bin/activate

pip3 install -r tests/requirements.txt

cd tests/PySys/

# Don't use -V this will might reveal secret credentials

#pysys.py run --record

#pysys.py run --record -c 20 c8y_restart_bridge
pysys.py run --record -c 100 smoketest_json

deactivate
