#!/bin/sh

# This script prints "command" and "service" to stdout.
# USAGE
# Input: $ ./dummy_init.sh restart mosquitto.service
# Output: restart mosquitto.service

COMMAND="$1"

case "$COMMAND" in
    is_available)
        echo is_available | sudo tee -a /etc/tedge/dummy_init.out > /dev/null
        ;;
    restart)
        echo restart $2 | sudo tee -a /etc/tedge/dummy_init.out > /dev/null
        ;;
    stop)
        echo stop $2 | sudo tee -a /etc/tedge/dummy_init.out > /dev/null
        ;;
    enable)
        echo enable $2 | sudo tee -a /etc/tedge/dummy_init.out > /dev/null
        ;;
    disable)
        echo disable $2 | sudo tee -a /etc/tedge/dummy_init.out > /dev/null
        ;;
    is-active)
        echo is-active $2 | sudo tee -a /etc/tedge/dummy_init.out > /dev/null
        ;;
    *)
        echo "Error: unsupported command: $COMMAND." | sudo tee -a /etc/tedge/dummy_init.out > /dev/null
        exit 1
        ;;
esac
