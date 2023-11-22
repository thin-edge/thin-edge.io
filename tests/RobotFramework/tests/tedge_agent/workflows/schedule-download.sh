#!/bin/sh
set -e

CMD_ID=$1

echo schedule download command target="$2" >>"/tmp/download-$CMD_ID"
echo '{"status":"scheduled"}'
