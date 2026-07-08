#!/bin/sh
# Initialize the PKCS#11 token used by tedge-p11-server, if it is not already initialized.
#
# This is a container-focused companion to configuration/contrib/pkcs11/tedge-init-hsm.sh (which is
# host/systemd oriented and also creates the key via `tedge cert create-key-hsm`). Here we only
# initialize the *token* (create the slot + set the user PIN). The device key is created later by
# the tedge client with `tedge cert create-key-hsm`, whose request is proxied to this server over
# the socket (the server has direct module access and performs the key generation on the token).
#
# The HSM type is derived from the configured module path, or can be forced with HSM_TYPE.
# Supported for auto-init: tpm2 (via p11tool) and softhsm (via softhsm2-util). USB tokens are not
# auto-initialized (initialization is device-specific and could be destructive).
#
# Relevant environment variables:
#   TEDGE_DEVICE_CRYPTOKI_MODULE_PATH  PKCS#11 module (used to detect the HSM type)
#   TEDGE_DEVICE_CRYPTOKI_PIN          user PIN to set on the token          (default: 123456)
#   P11_SO_PIN                         security-officer PIN                  (default: 12345678)
#   P11_TOKEN_LABEL                    label for the token                   (default: tedge)
#   HSM_TYPE                           override detection: tpm2|softhsm|usb
#   TPM2_PKCS11_STORE / SOFTHSM2_CONF  store locations (set in the Dockerfile)
set -e

MODULE="${TEDGE_DEVICE_CRYPTOKI_MODULE_PATH:-}"
PIN="${TEDGE_DEVICE_CRYPTOKI_PIN:-123456}"
SO_PIN="${P11_SO_PIN:-12345678}"
LABEL="${P11_TOKEN_LABEL:-tedge}"
HSM_TYPE="${HSM_TYPE:-}"

log() { echo "[init-hsm] $*" >&2; }

# Detect the HSM type from the module path when not explicitly set.
if [ -z "$HSM_TYPE" ]; then
    case "$MODULE" in
        *tpm2*)     HSM_TYPE=tpm2 ;;
        *softhsm*)  HSM_TYPE=softhsm ;;
        *opensc*)   HSM_TYPE=usb ;;
        *)          HSM_TYPE=unknown ;;
    esac
fi

# Idempotency: is a token with our label already present?
token_exists() {
    if command -v p11tool >/dev/null 2>&1; then
        p11tool ${MODULE:+--provider="$MODULE"} --list-tokens 2>/dev/null \
            | grep -q "token=${LABEL}\b" && return 0
    fi
    # Fallback for softhsm without p11tool
    if command -v softhsm2-util >/dev/null 2>&1; then
        softhsm2-util --show-slots 2>/dev/null | grep -q "Label:[[:space:]]*${LABEL}\b" && return 0
    fi
    return 1
}

if token_exists; then
    log "token '${LABEL}' already initialized; nothing to do"
    exit 0
fi

log "initializing token '${LABEL}' for HSM type '${HSM_TYPE}'"
case "$HSM_TYPE" in
    softhsm)
        softhsm2-util --init-token --free --label "$LABEL" --pin "$PIN" --so-pin "$SO_PIN"
        ;;
    tpm2)
        mkdir -p "${TPM2_PKCS11_STORE:-/etc/tedge/hsm}"
        # Pick the first available (uninitialized) token slot exposed by the module.
        TOKEN_URL=$(p11tool ${MODULE:+--provider="$MODULE"} --list-token-urls 2>/dev/null \
            | grep -v "token=${LABEL}\b" | head -n1)
        if [ -z "$TOKEN_URL" ]; then
            log "ERROR: no TPM token slot found (is the TPM device passed through, e.g. /dev/tpmrm0?)"
            exit 1
        fi
        log "using token slot: $TOKEN_URL"
        # Create the token/slot and set the PINs (see contrib/pkcs11/tedge-init-hsm.sh).
        GNUTLS_PIN="$PIN" GNUTLS_SO_PIN="$SO_PIN" \
            p11tool ${MODULE:+--provider="$MODULE"} --initialize --label "$LABEL" "$TOKEN_URL"
        GNUTLS_PIN="$PIN" GNUTLS_SO_PIN="$SO_PIN" \
            p11tool ${MODULE:+--provider="$MODULE"} --initialize-pin "pkcs11:token=${LABEL}"
        ;;
    usb)
        log "auto-init is not supported for USB tokens (initialize the device manually); skipping"
        exit 0
        ;;
    *)
        log "unknown HSM type for module '${MODULE}'; skipping auto-init"
        exit 0
        ;;
esac
log "token '${LABEL}' initialized"
