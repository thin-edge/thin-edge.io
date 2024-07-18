#!/bin/sh
set -e
SERVICE_NAME="$1"
USER_MESSAGE="$2"
sleep 10
systemctl restart "$SERVICE_NAME"
sleep 2
echo "$USER_MESSAGE"
