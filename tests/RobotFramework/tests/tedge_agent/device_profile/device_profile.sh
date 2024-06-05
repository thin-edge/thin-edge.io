#!/bin/sh
set -e

EXIT_OK=0

STATUS="$1"
shift
PAYLOAD=

example_payload() {
  CONFIG_URL="${1:-http://127.0.0.1:8001/c8y/inventory/binaries/35861751}"

  # Convert url to a local url
  case "$CONFIG_URL" in
    */inventory/binaries/*)
      echo "Converting config url to a c8y local proxy url" >&2
      MO_ID=$(echo "$CONFIG_URL" | rev | cut -d'/' -f1 | rev)
      CONFIG_URL="http://127.0.0.1:8001/c8y/inventory/binaries/$MO_ID"
      ;;
  esac

  cat << EOT
{
  "status": "init",
  "profile": [
    {
      "operation": "firmware_update",
      "skip": false,
      "payload": {
        "name": "core-image-tedge-rauc",
        "remoteUrl": "https://127.0.0.1:8000/some/dummy/url",
        "version": "20240430.1139"
      }
    },
    {
      "operation": "software_update",
      "skip": false,
      "payload": {
        "updateList": [
          {
            "type": "apt",
            "modules": [
              {
                "name": "c8y-command-plugin",
                "version": "latest",
                "action": "install"
              },
              {
                "name": "jq",
                "version": "latest",
                "action": "install"
              }
            ]
          }
        ]
      }
    },
    {
      "operation": "config_update",
      "skip": false,
      "payload": {
          "type": "tedge-configuration-plugin",
          "remoteUrl":"$CONFIG_URL"
      }
    }
  ]
}
EOT
}

if [ $# -eq 0 ]; then
    # Test operation to help with initial creation and debugging
    PAYLOAD="$(example_payload)"
else
    PAYLOAD="$1"
fi

log () { echo "$*" >&2; }
fail () { log "$@"; exit 1; }

update_state() {
    echo ':::begin-tedge:::'
    jo -- "$@"
    echo ':::end-tedge:::'
}

create_test_operation() {
    TOPIC="$1"
    CONFIG_URL="$2"
    PAYLOAD="$(example_payload "$CONFIG_URL")"
    tedge mqtt pub -r "$TOPIC" "$PAYLOAD"
}

scheduled() {
    log "Filtering/sorting device profile artifacts"

    # TODO: Filter/sort the artifacts
    ARTIFACTS=$(echo "$PAYLOAD" | jq '[ .profile[] | select(.skip != true) ]')
    update_state currentIndex="-1" profile="$ARTIFACTS"
}

next_artifact() {
    log "Checking next artifact"

    if [ $# -gt 0 ]; then
        PAYLOAD="$1"
        shift
    fi

    ARTIFACT_INDEX=$(echo "$PAYLOAD" | jq -r ".currentIndex // -1")
    NEXT_ARTIFACT_INDEX=$((ARTIFACT_INDEX + 1))

    CURRENT_ARTIFACT=$(echo "$PAYLOAD" | jq ".profile[$NEXT_ARTIFACT_INDEX]")

    if [ "$CURRENT_ARTIFACT" = "null" ]; then
        log "No more artifacts to process"
        # No more artifacts to process
        update_state status=successful
        exit "$EXIT_OK"
    fi

    NEXT_STATUS=$(echo "$PAYLOAD" | jq -r ".profile[$NEXT_ARTIFACT_INDEX].operation")

    # Prepare command
    log "Found next artifiact. status=$NEXT_STATUS"
    update_state status="$NEXT_STATUS" currentIndex="$NEXT_ARTIFACT_INDEX" current="$CURRENT_ARTIFACT"
}

case "$STATUS" in
    scheduled) scheduled "$@";;
    next_artifact) next_artifact "$@";;
    create_test_operation) create_test_operation "$@";;
    *)
        fail "Unknown status. status=$STATUS"
        ;;
esac

exit "$EXIT_OK"
