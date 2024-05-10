#!/bin/sh
set -e

COMMAND="$1"
shift

update_agent() {
    cat << EOT
:::begin-tedge:::
{
    "updateList": [
        {
            "type": "apt",
            "modules": [
                {
                    "name": "tedge-full",
                    "version": "latest",
                    "action": "install"
                }
            ]
        }
    ]
}
:::end-tedge:::
EOT
}

case "$COMMAND" in
    update_agent) update_agent "$@" ;;
esac
exit 0
