#!/bin/sh
set -e

CMD_ID="$1"
STATUS="$2"
REASON="$3"

echo restart "$STATUS" >>"/etc/tedge/operations/restart-$CMD_ID"

case "$STATUS" in
  "init") echo '{"status":"scheduled"}';;
  "successful_restart") echo '{"status":"successful"}';;
  "failed_restart") echo '{"status":"failed", "reason":"'"$REASON"'"}';;
  *) echo '{"status":"failed", "reason":"unknown state"}';;
esac
