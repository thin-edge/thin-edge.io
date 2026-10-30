#!/bin/sh
# Software plugin whose list can be made slow, for the agent to be restarted mid-request.
# The list is only slow while /tmp/slow-list exists, so the plugin check done by the agent on startup isn't delayed.

case "$1" in
    list)
        if [ -f /tmp/slow-list ]; then
            touch /tmp/slow-list-started
            sleep 10
        fi
        printf 'slow-package\t1.0.0\n'
        ;;
esac
exit 0
