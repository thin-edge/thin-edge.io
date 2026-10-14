#!/usr/bin/env bash
# Fails if a test function that was added or changed since BASE has a `///` doc comment.
# A test name should say what the test checks, so it needs no doc comment.
# Only changed lines are checked, so existing tests are reported once someone edits them.
#
# Usage: ci/check_test_doc_comments.sh BASE
# BASE is the commit to compare the working tree against, e.g. "$(git merge-base main HEAD)"
set -euo pipefail

BASE="${1:?usage: $0 BASE}"
status=0

while IFS= read -r file; do
    # Added line ranges as "start count" pairs; a missing count means 1
    added=$(git diff -U0 "$BASE" -- "$file" | sed -nE 's/^@@ -[0-9,]+ \+([0-9]+)(,([0-9]+))? @@.*/\1 \3/p' | awk '{ print $1, ($2 == "" ? 1 : $2) }')

    # Walks each run of doc comments and attributes in front of a `fn`,
    # tracking bracket depth so multi-line attributes like `#[test_case(...)]` stay in the run
    awk -v added="$added" '
        BEGIN {
            n = split(added, a, /[ \n]+/)
            for (i = 1; i < n; i += 2) for (l = a[i]; l < a[i] + a[i + 1]; l++) changed[l] = 1
        }
        function brackets(s,   opening, closing) {
            opening = gsub(/\[/, "", s); closing = gsub(/\]/, "", s); return opening - closing
        }
        depth > 0 { depth += brackets($0); next }
        /^[ \t]*\/\/\/([^\/]|$)/ { if (!run) { run = 1; start = FNR }; doc = 1; next }
        /^[ \t]*#\[/ {
            if (!run) { run = 1; start = FNR }
            if ($0 ~ /^[ \t]*#\[(tokio::)?test|^[ \t]*#\[rstest/) test = 1
            depth = brackets($0)
            next
        }
        run {
            if (doc && test && /(^|[ \t])fn[ \t]/) {
                for (l = start; l <= FNR; l++) if (l in changed) {
                    printf "%s:%d: doc comment on a test function; put the meaning in the test name instead\n", FILENAME, start
                    found = 1
                    break
                }
            }
            run = doc = test = 0
        }
        END { exit found }
    ' "$file" || status=1
done < <(git diff --name-only --diff-filter=AMR "$BASE" -- '*.rs')

exit "$status"
