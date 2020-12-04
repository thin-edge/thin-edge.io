#!/usr/bin/bash

#check the format of the rust code
if ! cargo fmt -- --check;
then
   exit 1
fi
# Lint Checking
if ! cargo clippy;
then
   exit 1
fi
# Run tests
if ! cargo test --verbose;
then
  exit 1
fi
# Build
if ! cargo build --release;
then
  exit 1
fi  
