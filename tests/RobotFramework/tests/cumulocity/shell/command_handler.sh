#!/bin/bash

info() {
    echo "$(date --iso-8601=seconds 2>/dev/null || date -Iseconds) : INFO : $*" >&2
}

# Parse the smart rest message, ignore the first two field, and everything afterwards is the command
COMMAND="${1#*,*,}"

# Check if command is wrapped with quotes, if so then remove them
if [[ "$COMMAND" == \"*\" ]]; then
    COMMAND="${COMMAND:1:-1}"
fi

info "Raw command: $*"
info "Executing command: $COMMAND"
bash -c "$COMMAND"
EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
    info "Command returned a non-zero exit code. code=$EXIT_CODE"
fi
exit $EXIT_CODE
