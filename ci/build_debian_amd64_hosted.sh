#!/bin/sh

set +x

# We use cargo deb to build debian packets, so we need to install it

cargo install cargo-deb
if [ $? -ne 0 ]; then exit 1; fi

rm -rf target/debian/*.deb
if [ $? -ne 0 ]; then exit 1; fi

# Build all debian packets

cargo deb -p tedge;
if [ $? -ne 0 ]; then exit 1; fi

cargo deb -p c8y_mapper;
if [ $? -ne 0 ]; then exit 1; fi

# Have a look what packets are there

ls -lah target/debian/
if [ $? -ne 0 ]; then exit 1; fi

