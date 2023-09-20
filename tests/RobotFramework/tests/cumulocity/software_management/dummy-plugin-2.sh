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

mkdir -p "/tmp/$PLUGIN_NAME"
cp "$FILEPATH" "/tmp/$PLUGIN_NAME/installed_$NAME"
