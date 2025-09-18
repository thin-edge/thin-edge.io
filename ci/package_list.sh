#!/bin/bash

# Note: Must be in the expected installation order
RELEASE_PACKAGES=(
    tedge
    tedge-mapper
    tedge-flows
    tedge-agent
    tedge-watchdog
    tedge-apt-plugin
    c8y-remote-access-plugin
    c8y-firmware-plugin
    tedge-p11-server
)
export RELEASE_PACKAGES

# List of binaries which should be built
BINARIES=(
    tedge
    tedge-p11-server
)
export BINARIES
