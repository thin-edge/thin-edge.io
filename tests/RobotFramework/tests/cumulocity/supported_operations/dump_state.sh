#!/bin/sh
set -e

# The service is registered under its systemd unit name, the last segment of its topic id
UNIT="${1##*/}"

echo ":::begin-tedge:::"
printf '{"result":"%s"}\n' "$(systemctl is-active "$UNIT")"
echo ":::end-tedge:::"
