#!/bin/bash
# Parse the smart rest message, ignore the first two field, and everything afterwards is the command
COMMAND="${1#*,*,}"

# Check if command is wrapped with quotes, if so then remove them
if [[ "$COMMAND" == \"*\" ]]; then
    COMMAND="${COMMAND:1:-1}"
fi

# Execute command
bash -c "$COMMAND"
EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
    echo "Command returned a non-zero exit code. code=$EXIT_CODE" >&2
fi

exit "$EXIT_CODE"