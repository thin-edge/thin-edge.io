#!/bin/sh

# This script prints "command" and "service" to stdout.
# USAGE
# Input: $ ./dummy_init.sh restart mosquitto.service
# Output: restart mosquitto.service

COMMAND="$1"

case "$COMMAND" in
    is_available)
        echo is_available >> /tmp/dummy_init/dummy_init.out
        ;;
    restart)
        echo restart $2 >> /tmp/dummy_init/dummy_init.out
        ;;
    stop)
        echo stop $2 >> /tmp/dummy_init/dummy_init.out
        ;;
    enable)
        echo enable $2 >> /tmp/dummy_init/dummy_init.out
        ;;
    disable)
        echo disable $2 >> /tmp/dummy_init/dummy_init.out
        ;;
    is-active)
        echo is-active $2 >> /tmp/dummy_init/dummy_init.out
        ;;
    *)
        echo "Error: unsupported command: $COMMAND." >> /tmp/dummy_init/dummy_init.out
        exit 1
        ;;
esac
