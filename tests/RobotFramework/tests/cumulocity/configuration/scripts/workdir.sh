#!/bin/sh
set -eu

echo ":::begin-tedge:::"
tmpdir=$(mktemp -d)
echo "{\"workdir\": \"$tmpdir\"}"
echo ":::end-tedge:::"
