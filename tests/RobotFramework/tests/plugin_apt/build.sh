#!/usr/bin/env bash
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
nfpm package -f "$SCRIPT_DIR/nfpm-version1.yaml" --target "$SCRIPT_DIR/" --packager deb
nfpm package -f "$SCRIPT_DIR/nfpm-version2.yaml" --target "$SCRIPT_DIR/" --packager deb
