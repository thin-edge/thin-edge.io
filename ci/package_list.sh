#!/bin/bash

# Note: Must be in the expected installation order
RELEASE_PACKAGES=(
    tedge
    tedge-mapper
    tedge-agent
    tedge-watchdog
    tedge-apt-plugin
    c8y-remote-access-plugin
    c8y-firmware-plugin
)
export RELEASE_PACKAGES

# Deprecated packages are still built but not explicitly tested
# This allows users to still access the packages if needed, however
# it is only reserved for packages with a more public facing API
DEPRECATED_PACKAGES=()
export DEPRECATED_PACKAGES
