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

# Package which should only be dynamically compiled
RELEASE_PACKAGES_NON_STATIC=(
    # requires loading pkcs11 .so files
    tedge-p11-server
)
export RELEASE_PACKAGES_NON_STATIC

# Deprecated packages are still built but not explicitly tested
# This allows users to still access the packages if needed, however
# it is only reserved for packages with a more public facing API
DEPRECATED_PACKAGES=()
export DEPRECATED_PACKAGES
