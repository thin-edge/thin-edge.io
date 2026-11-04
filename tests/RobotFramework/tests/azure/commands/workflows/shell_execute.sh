#!/bin/bash

COMMAND="${1}"

TMP_OUTPUT=$(mktemp)

set +e
# This runs whatever command the direct method carried, as-is: on a device where this
# operation is installed, anyone who can invoke the "shell" direct method gets
# root-equivalent shell access. Fine for this fixture; do not copy into a production
# /etc/tedge/operations without narrowing the accepted command set.
bash -c "$COMMAND" >"$TMP_OUTPUT" 2>&1
EXIT_CODE=$?
set -e

echo :::begin-tedge:::
printf '{"result":%s,"exitCode":%s}\n' "$(jq -R -s '.' < "$TMP_OUTPUT")" "$EXIT_CODE"
echo :::end-tedge:::

rm -f "$TMP_OUTPUT"

exit "$EXIT_CODE"
