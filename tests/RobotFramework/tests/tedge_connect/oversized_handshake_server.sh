#!/bin/sh
# Serves a TLS handshake that is too large for rustls to buffer, standing in for a
# Cumulocity instance that answers with an over-long CertificateRequest.
#
# This is not a TLS implementation. rustls enforces its 64 KB handshake limit while
# buffering, before parsing or validating anything, so a socket that writes one
# well-framed but over-large handshake message is enough. No key or certificate is
# needed, and no ServerHello.
#
# Which of rustls' two limits is hit depends on the size the message declares:
#
#   buffer-full  declares 65534 bytes, within the limit, but the framing overhead means
#                the buffer fills before the message completes, giving the io::Error
#                "message buffer full"
#   too-large    declares 65536 bytes, over the limit on its own, giving the
#                rustls::Error HandshakePayloadTooLarge
#
# Usage: oversized_handshake_server.sh [buffer-full|too-large] [port]
set -eu

MODE="${1:-buffer-full}"
PORT="${2:-18883}"
WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

# Every byte below is written with `printf '%b'`, which expands escapes in its argument
# rather than in the format string. That takes `echo`-style octal, meaning `\0` followed
# by up to three octal digits
case "$MODE" in
    buffer-full)
        # A handshake message length is 3 bytes wide: 0x00 0xff 0xfe
        DECLARED_BYTES='\0000\0377\0376'
        DECLARED=65534
        ;;
    too-large)
        # 0x01 0x00 0x00
        DECLARED_BYTES='\0001\0000\0000'
        DECLARED=65536
        ;;
    *)
        echo "unknown mode: $MODE" >&2
        exit 2
        ;;
esac

CERTIFICATE_MESSAGE='\0013'
HANDSHAKE_RECORD='\0026'
TLS_1_2_VERSION='\0003\0003'
MAX_RECORD_PAYLOAD=16384

# The message itself: a Certificate handshake message whose payload is zeroes. An
# incomplete message is never parsed, and one rustls refuses outright is not parsed
# either, so the contents do not matter
{
    printf '%b' "$CERTIFICATE_MESSAGE$DECLARED_BYTES"
    head -c "$DECLARED" /dev/zero
} > "$WORKDIR/message"

# Fragment it across TLS records, each carrying at most MAX_RECORD_PAYLOAD bytes
total=$(wc -c < "$WORKDIR/message")
offset=0
while [ "$offset" -lt "$total" ]; do
    remaining=$((total - offset))
    if [ "$remaining" -gt "$MAX_RECORD_PAYLOAD" ]; then
        chunk=$MAX_RECORD_PAYLOAD
    else
        chunk=$remaining
    fi

    # A record length is 2 bytes wide, big endian
    high=$(printf '%03o' $((chunk / 256)))
    low=$(printf '%03o' $((chunk % 256)))
    printf '%b' "$HANDSHAKE_RECORD$TLS_1_2_VERSION\\0$high\\0$low"
    tail -c "+$((offset + 1))" "$WORKDIR/message" | head -c "$chunk"

    offset=$((offset + chunk))
done > "$WORKDIR/flight"

echo "serving $(wc -c < "$WORKDIR/flight") bytes on port $PORT, declaring $DECLARED" >&2

# One connection per client, and keep listening: `tedge connect` may reconnect. Leaving
# the connection open until the client closes it means the client reports the handshake
# rather than an unexpected end of file
while true; do
    nc -l "$PORT" < "$WORKDIR/flight" || true
done
