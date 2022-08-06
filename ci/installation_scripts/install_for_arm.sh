#!/bin/bash -x

set -euo pipefail

PKG_DIR=$1

# Load the package list as $EXTERNAL_ARM_PACKAGES, $RELEASE_PACKAGES
source ./ci/package_list.sh

# Install pre-required packages
sudo apt-get --assume-yes install "$(echo "${EXTERNAL_ARM_PACKAGES[*]}")"

# Install thin-edge packages
for PACKAGE in "${RELEASE_PACKAGES[@]}"
do
    sudo dpkg -i ./"$PKG_DIR"/"$PACKAGE"_0.*_armhf.deb
done

# Configure collectd
sudo cp "/etc/tedge/contrib/collectd/collectd.conf" "/etc/collectd/collectd.conf"

# Change downloaded binaries to executable for testing
chmod +x /home/pi/examples/sawtooth_publisher
chmod +x /home/pi/tedge_dummy_plugin/tedge_dummy_plugin
