#!/bin/sh
set -e

# Constants
OK=0

# Input arguments
MESSAGE="$1"
NAME=$(echo "$MESSAGE" | cut -d, -f 3)
SSID=$(echo "$MESSAGE" | cut -d, -f 4)
TYPE=$(echo "$MESSAGE" | cut -d, -f 5)

echo "Processing message: $MESSAGE"
echo "NAME: $NAME"
echo "SSID: $SSID"
echo "TYPE: $TYPE"
exit "$OK"
