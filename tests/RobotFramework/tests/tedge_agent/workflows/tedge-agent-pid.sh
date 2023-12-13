#!/bin/sh
set -e

case "$1" in
"probe")
    CUR_PID=$(systemctl show --property MainPID tedge-agent)
    echo ':::begin-tedge:::'
    echo '{"tedge-agent-pid": "'"$CUR_PID"'"}'
    echo ':::end-tedge:::'
    ;;

"test")
    OLD_PID="$2"
    NEW_PID=$(systemctl show --property MainPID tedge-agent)
    until test "$OLD_PID" != "$NEW_PID"
    do
        sleep 1
        NEW_PID=$(systemctl show --property MainPID tedge-agent)
    done
    echo ':::begin-tedge:::'
    echo '{"tedge-agent-pid": "'"$NEW_PID"'","old-tedge-agent-pid": "'"$OLD_PID"'"}'
    echo ':::end-tedge:::'
    ;;

*) exit 1;;
esac
