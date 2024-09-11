#!/bin/sh
set -e

preprocess_sqlite() {
    #
    # Create a log file in the location
    # that is defined in the tedge-log-plugin.toml
    #
    LOG_TYPE="$1"
    TEDGE_FTS_URL="$2"
    DATE_FROM="$3"
    DATE_TO="$4"
    SEARCH_TEXT="$5"

    TMP_LOG_FILE=/tmp/${LOG_TYPE}.log

    # delete any existing log file (from a previous run)
    rm -f "$TMP_LOG_FILE"

    # Run the query
    echo "Running sqlite query. dateFrom=$DATE_FROM, dateTo=$DATE_TO, searchText=$SEARCH_TEXT" >&2
    echo "File will be upload to $TEDGE_FTS_URL" >&2

    cat << EOT > "$TMP_LOG_FILE"
Running some sqlite query...
Parameters:
    dateFrom=$DATE_FROM
    dateTo=$DATE_TO
EOT
}

postprocess_sqlite() {
      LOG_TYPE="$1"
      TMP_LOG_FILE=/tmp/${LOG_TYPE}.log
      rm -f "$TMP_LOG_FILE"
}

#
# Main
#
if [ $# -lt 1 ]; then
    echo "Missing required positional argument" >&2
    exit 2
fi

COMMAND="$1"
shift

case "$COMMAND" in
    preprocess)
        LOG_TYPE="$1"
        case "$LOG_TYPE" in
            sqlite)
                preprocess_sqlite "$@"
                ;;
            *)
                echo "Log type does not require a pre-processing. type=$LOG_TYPE" >&2
                ;;
        esac
        ;;
    postprocess)
        postprocess_sqlite "$@"
        ;;
esac

exit 0
