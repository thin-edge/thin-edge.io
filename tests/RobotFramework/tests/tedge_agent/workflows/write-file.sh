#!/bin/sh
set -e

FILE="$1"
shift
CONTENT="$*"

if [ -n "$FILE" ]
then
    echo "$CONTENT" >>"$FILE"
fi