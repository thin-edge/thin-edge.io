#!/bin/bash -x

# Stop services
sudo systemctl stop tedge-mapper-collectd
sudo tedge disconnect c8y
sudo tedge disconnect az
sudo systemctl stop apama

# Load the release package list as $RELEASE_PACKAGES
# shellcheck disable=SC1091
source ./ci/package_list.sh

# Fix any broken packages caused by missing dependencies
sudo apt-get install -f -y

# Purge packages
sudo apt --assume-yes purge "${RELEASE_PACKAGES[@]}"
sudo DEBIAN_FRONTEND=noninteractive apt --assume-yes purge "${EXTERNAL_ARM_PACKAGES[@]}"
