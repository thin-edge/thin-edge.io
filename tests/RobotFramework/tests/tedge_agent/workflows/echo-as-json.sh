#!/bin/sh
set -e

echo ':::begin-tedge:::'
echo '{'
while [ -n "$1" ]
do
    KEY="$1"
    VAL="$2"
    shift
    shift
    if [ -n "$1" ]
    then
        SEP=","
    else
        SEP=""
    fi
    echo "\"$KEY\": \"$VAL\"$SEP"
done
echo '}'
echo ':::end-tedge:::'