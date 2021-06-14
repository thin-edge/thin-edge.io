SRC=docs/src/references
PATH=$PATH:target/debug:target/release

cat >$SRC/tedge.md <<EOF
# The \`tedge\` command

\`\`\`
$(tedge --help)
\`\`\`
EOF

cat >$SRC/tedge-cert.md <<EOF
# The \`tedge cert\` command

\`\`\`
$(tedge cert --help)
\`\`\`

## Create

\`\`\`
$(tedge cert create --help)
\`\`\`

## Show

\`\`\`
$(tedge cert show --help)
\`\`\`

## Remove

\`\`\`
$(tedge cert remove --help)
\`\`\`

## Upload

\`\`\`
$(tedge cert upload --help)
\`\`\`
EOF

cat >$SRC/tedge-config.md <<EOF
# The \`tedge config\` command

\`\`\`
$(tedge config --help)
\`\`\`

## Get

\`\`\`
$(tedge config get --help)
\`\`\`

## Set

\`\`\`
$(tedge config set --help)
\`\`\`

## List

\`\`\`
$(tedge config list --help)
\`\`\`

## Unset

\`\`\`
$(tedge config unset --help)
\`\`\`
EOF


cat >$SRC/tedge-connect.md <<EOF
# The \`tedge connect\` command

\`\`\`
$(tedge connect --help)
\`\`\`

## Azure

\`\`\`
$(tedge connect az --help)
\`\`\`

## Cumulocity

\`\`\`
$(tedge connect c8y --help)
\`\`\`
EOF

cat >$SRC/tedge-disconnect.md <<EOF
# The \`tedge disconnect\` command

\`\`\`
$(tedge disconnect --help)
\`\`\`

## Azure

\`\`\`
$(tedge disconnect az --help)
\`\`\`

## Cumulocity

\`\`\`
$(tedge disconnect c8y --help)
\`\`\`
EOF

cat >$SRC/tedge-mqtt.md <<EOF
# The \`tedge mqtt\` command

\`\`\`
$(tedge mqtt --help)
\`\`\`

## Pub

\`\`\`
$(tedge mqtt pub --help)
\`\`\`

## Sub

\`\`\`
$(tedge mqtt sub --help)
\`\`\`
EOF
