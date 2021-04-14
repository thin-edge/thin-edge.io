#!/usr/bin/bash

# Run all available feature-tests.
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

set -e

#export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=/usr/bin/arm-linux-gnueabihf-gcc
export CARGO_TARGET_ARMV_UNKNOWN_LINUX_GNUEABIHF_LINKER=/usr/bin/arm-linux-gnueabihf-gcc

cd $TEBASEDIR
cd common/mqtt_client
cargo test --features integration-test

cd $TEBASEDIR
cd mapper/cumulocity/c8y_translator_lib
cargo test --features integration-test
