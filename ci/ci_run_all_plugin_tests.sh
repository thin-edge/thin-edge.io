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

set -e

cd $TEBASEDIR

# Check if clients are installed
dpkg -s mosquitto-clients

# Run all PySys tests

python3 -m venv ~/env-pysys
source ~/env-pysys/bin/activate
pip3 install -r tests/requirements.txt
cd tests/PySys/

sudo tedge config set software.plugin.default apt

pysys.py run -v DEBUG 'apt_*' -XmyPlatform='container'

sudo cp ../../plugins/tedge_docker_plugin/tedge_docker_plugin.sh /etc/tedge/sm-plugins/docker

pysys.py run -v DEBUG 'docker_*' -XmyPlatform='container' -Xdockerplugin='dockerplugin'

sudo rm -f /etc/tedge/sm-plugins/docker

sudo tedge config unset software.plugin.default

deactivate
