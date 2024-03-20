#!/bin/sh
set -e

FILE="$1"
CONTENT="$2"

if [ -n "$FILE" ]
then
    echo "$CONTENT" >"$FILE"
fi