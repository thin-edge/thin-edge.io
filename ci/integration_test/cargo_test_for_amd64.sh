#!/bin/bash -x

set -euo pipefail

# Compile in advance to avoid that cargo compiles during the test run
# this seems to have an impact on some tests as the timing differs
cargo test --verbose --no-run --features integration-test
cargo build -p tedge_dummy_plugin
