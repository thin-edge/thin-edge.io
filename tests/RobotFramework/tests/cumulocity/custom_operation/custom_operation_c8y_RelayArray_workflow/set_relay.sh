#!/bin/sh

MESSAGE="$1"

echo :::begin-tedge:::
printf '{"result":%s}\n' "$MESSAGE"
echo :::end-tedge:::
