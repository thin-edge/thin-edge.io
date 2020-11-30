#!/usr/bin/bash

#check the format of the rust code 
cargo fmt -- --check
# Lint Checking
cargo clippy
# Run tests
cargo test --verbose
# Build
cargo build --release

