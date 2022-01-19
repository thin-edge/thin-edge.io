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

sudo cp plugins/tedge_docker_plugin/tedge_docker_plugin.sh /etc/tedge/sm-plugins/docker

# Assume that the pi user has a precompiled dummy plugin
# use a standard path for the others
if [ $(whoami) == "pi" ];
  then
    sudo cp /home/pi/tedge_dummy_plugin/tedge_dummy_plugin /etc/tedge/sm-plugins/fruits
  else
    sudo cp $HOME/thin-edge.io/target/release/tedge_dummy_plugin /etc/tedge/sm-plugins/fruits
fi

sudo chmod +x /etc/tedge/sm-plugins/docker
sudo chmod +x /etc/tedge/sm-plugins/fruits

sudo tedge config set software.plugin.default apt

sudo mkdir -p /tmp/.tedge_dummy_plugin/

sudo cp tests/PySys/software_management_end_to_end/dummy_plugin_configuration/list-valid.0 /tmp/.tedge_dummy_plugin/list-valid.0

# Run all PySys tests

python3 -m venv ~/env-pysys
source ~/env-pysys/bin/activate
pip3 install -r tests/requirements.txt
cd tests/PySys/

# Run all software management tests, including the ones for the
# fake- and the  docker plugin
set +e
pysys.py run --record -v DEBUG 'SoftwareManagement.*' -XmyPlatform='smcontainer' -Xdockerplugin='dockerplugin' -Xfakeplugin='fakeplugin'
set -e

deactivate

sudo tedge config unset software.plugin.default
sudo rm -f /etc/tedge/sm-plugins/docker
sudo rm -f /etc/tedge/sm-plugins/fruits

mv __pysys_junit_xml pysys_junit_xml_sm
