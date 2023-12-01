#!/bin/sh
set -e

CMD_ID=$1

echo check download command outcome="$2" >>"/tmp/download-$CMD_ID"

echo ':::begin-tedge:::'
echo '{"status":"successful"}'
echo ':::end-tedge:::'
