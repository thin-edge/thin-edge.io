#!/bin/sh
set -e

COMMIT_HOOK=.git/hooks/prepare-commit-msg
echo "Adding git prepare-commit hook to append --sign-off to all commits. file: $COMMIT_HOOK" >&2

cat <<"EOT" > "$COMMIT_HOOK"
#!/bin/sh

NAME=$(git config user.name)
EMAIL=$(git config user.email)

if [ -z "$NAME" ]; then
    echo "empty git config user.name"
    exit 1
fi

if [ -z "$EMAIL" ]; then
    echo "empty git config user.email"
    exit 1
fi

git interpret-trailers --if-exists doNothing --trailer \
    "Signed-off-by: $NAME <$EMAIL>" \
    --in-place "$1"
EOT

chmod +x .git/hooks/prepare-commit-msg
