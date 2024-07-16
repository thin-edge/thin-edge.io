#!/bin/sh
set -e

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 '<json-string>'"
  exit 1
fi

echo :::begin-tedge:::
echo "$1" | jq '.["@next"].operation.payload'
echo :::end-tedge:::
