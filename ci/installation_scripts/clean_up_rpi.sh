#!/bin/bash -x

# Stop services
sudo systemctl stop tedge-mapper-collectd
sudo tedge disconnect c8y
sudo tedge disconnect az
sudo systemctl stop apama

# Load the release package list as $RELEASE_PACKAGES
source ./ci/package_list.sh

# Purge packages
sudo apt --assume-yes purge "$(echo "${RELEASE_PACKAGES[*]}")"
sudo DEBIAN_FRONTEND=noninteractive apt --assume-yes purge "$(echo "${EXTERNAL_ARM_PACKAGES[*]}")"
