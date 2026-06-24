#!/bin/bash
set -euxo pipefail

case "$1" in
  list)
    echo huge
    ;;
  get)
    if [ "$2" = "huge" ]; then
      # Emit ~50MB to stdout to simulate an oversized logfile
      len=50M
      dd if=/dev/zero bs=$len count=1 2>/dev/null | tr '\0' 'A'

      # once the entire log is generated output a message so we know we can measure memory used
      echo "script generated $len of data" >&2

      # create a pipe and read it to pause executing, allowing test to measure memory usage and
      # decide when the script should exit
      mkfifo /tmp/huge-plugin-pipe
      read -r _x < /tmp/huge-plugin-pipe

      # finally exit with non-zero code so data isn't sent to c8y unnecessarily
      rm /tmp/huge-plugin-pipe
      exit 67
    else
      echo "Unknown log type" >&2
      exit 2
    fi
    ;;
  *)
    echo "Usage: $0 {list|get <type>}" >&2
    exit 2
    ;;
esac
