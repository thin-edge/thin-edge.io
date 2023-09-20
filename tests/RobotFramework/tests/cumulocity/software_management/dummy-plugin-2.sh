#!/bin/bash

set -eo pipefail

PLUGIN_NAME=$(basename "$0")

case "$1" in
  list)
      exit 0
      ;;
  prepare)
      exit 0
      ;;
  update-list)
      while IFS= read -r line || [[ -n $line ]]; do
        echo "Executing: $0 $line"
        # shellcheck disable=SC2086
        "$0" $line
      done
      exit 0
      ;;
  finalize)
      exit 0
      ;;
  install)
      ;;
  *)
     exit 0
     ;;
esac

shift

NAME="$1"
shift

while [[ $# -gt 0 ]]; do
  case $1 in
    --file)
      FILEPATH="$2"
      shift # past argument
      shift # past value
      ;;
    --*)
      shift # past argument
      shift # past value
      ;;
    *)
      echo "Unknown argument $1"
      exit 1
      ;;
  esac
done

if [[ $(cat "$FILEPATH") != "Testing a thing" ]]; then
    echo -e "Downloaded file (for package $NAME) does not have expected SHA256. Contents are:\n\n$(cat "$FILEPATH")"
    exit 1
else
    mkdir -p "/tmp/$PLUGIN_NAME"
    touch "/tmp/$PLUGIN_NAME/intalled_$NAME"
fi

