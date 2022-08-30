#!/bin/bash -x

set -euo pipefail

PKG_DIR=$1

# Load the package list as $EXTERNAL_AMD64_PACKAGES and $RELEASE_PACKAGES
# shellcheck disable=SC1091
source ./ci/package_list.sh

# Install pre-required packages
sudo apt-get --assume-yes install "${EXTERNAL_AMD64_PACKAGES[@]}"

# Install thin-edge packages
for PACKAGE in "${RELEASE_PACKAGES[@]}"
do
    sudo dpkg -i ./"$PKG_DIR"/"$PACKAGE"_0.*_amd64.deb
done
