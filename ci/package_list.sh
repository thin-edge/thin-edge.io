#!/bin/bash

# Note: Must be in the expected installation order
RELEASE_PACKAGES=(
    tedge
    tedge_mapper
    tedge_agent
    tedge_watchdog
    tedge_apt_plugin
    tedge_apama_plugin
    c8y_log_plugin
    c8y_configuration_plugin
)
export RELEASE_PACKAGES

TEST_PACKAGES=(
    sawtooth_publisher
    tedge_dummy_plugin
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

