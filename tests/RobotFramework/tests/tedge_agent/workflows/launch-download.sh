#!/bin/sh
set -e

CMD_ID=$1

echo launch download url="$2" file="$3" >>"/tmp/download-$CMD_ID"

echo ':::begin-tedge:::'
echo '{"tmp": "/tmp/download/'"$CMD_ID"'"}'
echo ':::end-tedge:::'