#!/bin/sh
set -e

echo new software list request topic = "$1" >>/tmp/operations.log
echo '{ "status":"scheduled" }'
