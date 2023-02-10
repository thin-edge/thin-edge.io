#!/bin/bash

# Note: Must be in the expected installation order
RELEASE_PACKAGES=(
    tedge
    tedge-mapper
    tedge-agent
    tedge-watchdog
    tedge-apt-plugin
    c8y-log-plugin
    c8y-configuration-plugin
    c8y-remote-access-plugin
    c8y-firmware-plugin
)
export RELEASE_PACKAGES

TEST_PACKAGES=(
    sawtooth-publisher
    tedge-dummy-plugin
)
export TEST_PACKAGES

EXTERNAL_ARM_PACKAGES=(
    mosquitto-clients
    mosquitto
    libmosquitto1
    collectd-core
    collectd
)
export EXTERNAL_ARM_PACKAGES

EXTERNAL_AMD64_PACKAGES=(
    mosquitto
    libmosquitto1
    collectd-core
)
export EXTERNAL_AMD64_PACKAGES

