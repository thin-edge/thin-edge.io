#!/bin/bash

# Load config
source /etc/hello.conf

touch /var/log/hello.log

while true; do
    echo "$(date) - $MESSAGE" >> /var/log/hello.log
    sleep 1
done