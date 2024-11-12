#!/bin/sh
set -e

DURATION=$1
QUALITY=$2

echo Took it. DURATION="$DURATION", QUALITY="$QUALITY"

echo ':::begin-tedge:::'
echo '{"status":"successful"}'
echo ':::end-tedge:::'
