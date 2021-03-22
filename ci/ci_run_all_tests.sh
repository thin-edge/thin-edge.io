#!/usr/bin/bash

# Run all available system-tests.
# Note: Needs a bash shell to run
#
# Expected environment variables to be set:
# C8YPASS : Cumulocity password
# C8YUSERNAME : Cumolocity username
# C8YTENANT : Cumolocity tennant
# C8YDEVICE : The device name
# C8YDEVICEID : The device ID in Cumolocity
# TIMEZONE : Your timezone (temporary)
# TEBASEDIR : Base directory for the thin edge repo
# EXAMPLEDIR : The direcory of the sawtooth example

set -e

cd $TEBASEDIR

# Run all PySys tests

python3 -mvenv ~/env-pysys
source ~/env-pysys/bin/activate
pip3 install -r tests/requirements.txt
cd tests/PySys/
pysys.py run -v DEBUG
deactivate
