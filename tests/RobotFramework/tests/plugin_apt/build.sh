#!/usr/bin/env bash
set -e
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
pushd "$SCRIPT_DIR" 2>/dev/null ||:
nfpm package -f "nfpm-version1.yaml" --target "./" --packager deb
nfpm package -f "nfpm-version2.yaml" --target "./" --packager deb
nfpm package -f "nfpm-package-with-epoch.yaml" --target "./" --packager deb
popd 2>/dev/null ||:
