#!/bin/sh
set -e

ARG1=$1
ARG2=$2

echo ':::begin-tedge:::'
printf '{"something":"%s %s"}\n' "$ARG1" "$ARG2"
echo ':::end-tedge:::'
