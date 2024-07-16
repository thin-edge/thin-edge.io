#!/bin/sh
set -e

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 '<json-string>'"
  exit 1
fi

json_input="$1"

echo "$json_input" | jq '(.next = .operations[0])'
