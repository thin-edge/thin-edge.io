#!/bin/sh
#
# Dummy set relay array option which just returns the status given the input desired state
# after a short delay
#
# Message format:
#     519,<serial>,<state>,<state>,...
#     519,tedge,CLOSED,OPEN,OPEN
#
MESSAGE="$1"
DESIRED_STATE=$(echo "$MESSAGE" | cut -d, -f3-)
sleep 1
# Return current state (e.g. the desired)
printf '%s' "$DESIRED_STATE"
