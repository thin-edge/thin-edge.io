#!/bin/sh
set -e

CMD_ID=$1
OUTCOME=$(echo "$2" | jq -c 'del(.payload."@time")')

echo check download command outcome="$OUTCOME" >>"/tmp/download-$CMD_ID"

echo ':::begin-tedge:::'
echo '{"status":"successful"}'
echo ':::end-tedge:::'
