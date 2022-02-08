#!/usr/bin/bash

# Run all available system-tests.
# Note: Needs a bash shell to run
#
# Expected environment variables to be set:
# C8YPASS : Cumulocity password
# C8YUSERNAME : Cumolocity username
# C8YTENANT : Cumolocity tenant
# C8YDEVICE : The device name
# TIMEZONE : Your timezone (temporary)
# TEBASEDIR : Base directory for the Thin-Edge repo
# EXAMPLEDIR : The direcory of the sawtooth example
# C8YURL : e.g. https://thin-edge-io.eu-latest.cumulocity.com

set -e

export C8YDEVICEID=$(./ci/find_device_id.py --tenant $C8YTENANT --user $C8YUSERNAME --device $C8YDEVICE --url $URL)

cd $TEBASEDIR

# Check if clients are installed
dpkg -s mosquitto-clients

# Run all PySys tests

python3 -m venv ~/env-pysys
source ~/env-pysys/bin/activate
pip3 install -r tests/requirements.txt
cd tests/PySys/

sudo tedge config set software.plugin.default apt

set +e
pysys.py run --record -v DEBUG 'apt_*' -XmyPlatform='smcontainer'
set -e

mv __pysys_junit_xml pysys_junit_xml_apt

sudo cp ../../plugins/tedge_docker_plugin/tedge_docker_plugin.sh /etc/tedge/sm-plugins/docker
sudo chmod +x /etc/tedge/sm-plugins/docker

set +e
pysys.py run --record -v DEBUG 'docker_*' -XmyPlatform='container' -Xdockerplugin='dockerplugin'
set -e

mv __pysys_junit_xml pysys_junit_xml_docker

sudo rm -f /etc/tedge/sm-plugins/docker

sudo tedge config unset software.plugin.default

deactivate
