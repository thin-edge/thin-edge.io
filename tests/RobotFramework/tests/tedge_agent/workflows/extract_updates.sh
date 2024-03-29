#!/bin/sh
set -e

# Extract the combined set of items on which the given task is to be performed for a given step from the profile file.
# Refer to `lite_device_profile.expected.log` for a sample profile file.
# Examples:
#
# Input: $0 step_1 configure /path/to/lite_device_profile.example.txt
# Output: { "configure": ["mosquitto"]}
#
# Input: $0 step_2 install /path/to/lite_device_profile.example.txt
# Output: { "install": ["jq", "ripgrep"]}

STEP="$1"
TASK="$2"
PROFILE="$3"

echo ':::begin-tedge:::'
echo "{ \"$TASK\": ["
if [ -n "$PROFILE" ] && [ -n "$STEP" ] && [ -n "$TASK" ]
then
    SEP=""
    grep "$STEP" "$PROFILE" | grep "$TASK" | awk '{print $3}' | while read -r ITEM
    do
        echo "$SEP \"$ITEM\""
        SEP=",";
    done
fi
echo ']}'
echo ':::end-tedge:::'



