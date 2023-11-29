#!/bin/sh
set -e

echo new software list request topic = "$1" >>/tmp/operations.log

echo ':::begin-tedge:::'
echo '{ "status":"scheduled" }'
echo ':::end-tedge:::'
