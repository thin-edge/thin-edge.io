#!/bin/sh

echo new software list request topic = $1 >>/tmp/operations.log
echo new software list request payload = $2 >>/tmp/operations.log
echo '{ "status":"scheduled" }'
