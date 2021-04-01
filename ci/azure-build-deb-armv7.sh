#!/usr/bin/sh

set +x

# Check if the additional gcc is there
ls -lah /usr/bin/arm-linux-gnueabihf-gcc
if [ $? -ne 0 ]; then exit 1; fi


export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=/usr/bin/arm-linux-gnueabihf-gcc

# This is basically doing the same check as above.
# However, we might to switch to a different linker to fix stripping
# of debian packages

if [ ! -f "$CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER" ]; then
    echo "Error: $CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER does not exist."
    exit 1
fi

# Build all debian packages
# Stripping does not work yet

cargo deb -p tedge --no-strip --target=armv7-unknown-linux-gnueabihf
if [ $? -ne 0 ]; then exit 1; fi

cargo deb -p c8y_mapper --no-strip --target=armv7-unknown-linux-gnueabihf
if [ $? -ne 0 ]; then exit 1; fi

# We expect to find these files

TEDGE="/home/azureuser/actions-runner/_work/thin-edge.io/thin-edge.io/target/armv7-unknown-linux-gnueabihf/debian/tedge_0.1.0_armhf.deb"
MAPPER="/home/azureuser/actions-runner/_work/thin-edge.io/thin-edge.io/target/armv7-unknown-linux-gnueabihf/debian/c8y_mapper_0.1.0_armhf.deb"


# Have a look what packages are there

ls -lah target/armv7-unknown-linux-gnueabihf/debian/
if [ $? -ne 0 ]; then exit 1; fi

if [ ! -f "$TEDGE" ]; then
    echo "Error: $TEDGE does not exist."
    exit 1
fi

if [ ! -f "$MAPPER" ]; then
    echo "Error: $MAPPER does not exist."
    exit 1
fi
