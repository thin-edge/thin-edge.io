#!/bin/sh
set -e

ARG1=$1
ARG2=$2

echo ARG1="$ARG1", ARG2="$ARG2"

echo ':::begin-tedge:::'
echo '{"status":"successful"}'
echo ':::end-tedge:::'
