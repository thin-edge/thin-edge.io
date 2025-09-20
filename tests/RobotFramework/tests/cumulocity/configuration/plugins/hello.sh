#!/bin/bash

# Load config
# shellcheck source=/dev/null
source /etc/hello.conf

touch /var/log/hello.log

while true; do
    echo "$(date) - $MESSAGE" >> /var/log/hello.log
    sleep 1
done