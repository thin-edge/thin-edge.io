#!/bin/bash

info() {
    echo "$(date --iso-8601=seconds 2>/dev/null || date -Iseconds) : INFO : $*" >&2
}

# Parse the message
COMMAND="${1}"

TMP_OUTPUT=$(mktemp)
info "Writing command output to file. path=$TMP_OUTPUT"

EXIT_CODE=0

set +e
bash -c "$COMMAND" >"$TMP_OUTPUT" 2>&1
EXIT_CODE=$?
set -e

if [ "${EXIT_CODE}" -ne 0 ]; then
    info "Command returned a non-zero exit code. code=$EXIT_CODE"
fi

echo :::begin-tedge:::
printf '{"result":%s}\n' "$(jq -R -s '.' < "$TMP_OUTPUT")"
echo :::end-tedge:::

exit "$EXIT_CODE"
