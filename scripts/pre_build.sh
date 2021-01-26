#!/bin/bash

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
