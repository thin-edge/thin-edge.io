#!/bin/sh
set -e

TOKEN_URL="${TOKEN_URL:-}"

export GNUTLS_PIN="${GNUTLS_PIN:-123456}"
export GNUTLS_SO_PIN="${GNUTLS_SO_PIN:-12345678}"
export TOKEN_LABEL="${TOKEN_LABEL:-tedge}"
export TEDGE_CONFIG_DIR="${TEDGE_CONFIG_DIR:-/etc/tedge}"
export PUBLIC_KEY="${PUBLIC_KEY:-${TEDGE_CONFIG_DIR}/device-certs/tedge.pub}"

# private key options
KEY_TYPE=${KEY_TYPE:-}
RSA_BITS=${RSA_BITS:-}
ECDSA_CURVE=${ECDSA_CURVE:-}

# Only used for TPM 2.0
export TPM2_PKCS11_STORE="${TPM2_PKCS11_STORE:-/etc/tedge/hsm}"

PKCS11_MODULE="${PKCS11_MODULE:-}"

ACTION="create"

HSM_TYPE="${HSM_TYPE:-}"

usage() {
    cat <<EOT
Initialize a Hardware Security Module for usage with thin-edge.io.
The script will initialize the slot, and create a keypair which will
be used by the tedge components.

This is a convenience script to make it easier for users to perform
the initial setup. If you encounter any errors, please refer
to your HSM's manufacturer notes.

$0 [OPTIONS]

ARGUMENTS
  --type <string>           Type of HSM (using the PKCS#11 interface) to use. Available values: [tpm2, nitrokey, softhsm2]
  --token-url <url>         Token PKCS#11 URL which is to be used for initialization.
  --label <string>          Token label to be associated with the created key pair. Defaults to tedge
  --id <string>             Token id to be associated with the created key pair. Defaults to a randomized value
  --pin <string>            Pin used to access the HSM
  --so-pin <string>         Special pin
  --module <path>           Path to the PKCS#11 module to use
  --key-type <ecdsa|rsa>    The type of the key, e.g. ecdsa, rsa
  --curve <p256|p384>       The curve (size) of the ECDSA key, e.g. p256, p384
  --bits <2048|3072|4096>   The size of the RSA keys in bits, e.g. 2048, 3072, 4096
  --debug                   Enable debugging
  -h, --help                Show this help

EXAMPLES

The following examples detail how to initialize different types
of HSMs.

## TPM2

$0 --type tpm2 --pin $GNUTLS_PIN --so-pin $GNUTLS_SO_PIN --token-url 'pkcs11:model=SLB9672%00%00%00%00%00%00%00%00%00;manufacturer=Infineon;serial=0000000000000000;token='
# Initialize a new slot and create a new private key pair in a TPM 2.0 module

## Nitrokey

$0 --type nitrokey --pin $GNUTLS_PIN --so-pin $GNUTLS_SO_PIN --token-url 'pkcs11:model=PKCS%2315%20emulated;manufacturer=www.CardContact.de;serial=DENK0400089;token=SmartCard-HSM%20%28UserPIN%29'
# Initialize a new slot and create a new private key pair using a nitrokey (USB based HSM)


## SoftHSM2

$0 --type softhsm2 --pin $GNUTLS_PIN --so-pin $GNUTLS_SO_PIN
# Initialize a new slot and create a new private key pair using softhsm2 (for testing only)

EOT
}

#
# Parse arguments
#
while [ $# -gt 0 ]; do
    case "$1" in
        --type)
            HSM_TYPE="$2"
            shift
            ;;
        --module)
            PKCS11_MODULE="$2"
            shift
            ;;
        --label)
            TOKEN_LABEL="$2"
            shift
            ;;
        --id)
            TOKEN_ID="$2"
            shift
            ;;
        --token-url)
            TOKEN_URL="$2"
            shift
            ;;
        --pin)
            GNUTLS_PIN="$2"
            shift
            ;;
        --so-pin)
            GNUTLS_SO_PIN="$2"
            shift
            ;;
        --key-type)
            KEY_TYPE="$2"
            shift
            ;;
        --bits)
            RSA_BITS="$2"
            KEY_TYPE="rsa"
            shift
            ;;
        --curve)
            ECDSA_CURVE="$2"
            KEY_TYPE="ecdsa"
            shift
            ;;
        --debug)
            set -x
            ;;
        --help|-h)
            usage
            exit 0
            ;;
    esac
    shift
done

if [ -z "$PKCS11_MODULE" ]; then
    VALUE=$(tedge config get device.cryptoki.module_path 2>/dev/null ||:)
    if [ -n "$VALUE" ]; then
        if [ -f "$VALUE" ]; then
            PKCS11_MODULE="$VALUE"
        else
            tedge config unset device.cryptoki.module_path 2>/dev/null ||:
        fi
    fi
fi

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

show_usage_and_fail() {
    echo "ERROR: $*" >&2
    usage
    exit 1
}

if [ "$(id -u)" -ne 0 ]; then
    fail "Script must be run as root"
fi

if [ -z "$HSM_TYPE" ]; then
    show_usage_and_fail "You must provide the --type <value> flag to indicate which hsm type you would like to use"
fi

# Set module defaults
find_pkcs11_module() {
    if [ -n "$PKCS11_MODULE" ]; then
        # module is already set
        return
    fi

    case "$HSM_TYPE" in
        softhsm2|softhsm)
            PKCS11_MODULE=$(find /usr/lib -name libsofthsm2.so | head -n1)
            ;;
        nitrokey)
            PKCS11_MODULE=$(find /usr/lib -name opensc-pkcs11.so | head -n1)
            ;;
        tpm2)
            PKCS11_MODULE=$(find /usr/lib -name libtpm2_pkcs11.so | head -n1)
            ;;
        *)
            # Don't use an explicit pkcs11 module, let the tooling choose the default
            ;;
    esac
}

#
# Enable usage with thin-edge.io
#

configure_tedge() {
    tedge config set mqtt.bridge.built_in true
    tedge config set device.cryptoki.mode socket
    tedge config set device.cryptoki.module_path "$PKCS11_MODULE"
    tedge config set device.cryptoki.pin "$GNUTLS_PIN"
}

init_private_key() {
    # set common arguments to ensure p11tool finds the correct module if there are multiple
    P11_TOOL_ARGS=
    PKCS11_MODULE=$(tedge config get device.cryptoki.module_path ||:)
    if [ -n "$PKCS11_MODULE" ]; then
        P11_TOOL_ARGS="--provider=$PKCS11_MODULE"
    fi

    # Target token URL
    TEDGE_TOKEN_URL="pkcs11:token=$TOKEN_LABEL"

    case "$1" in
        nitrokey)
            if [ -z "$TOKEN_URL" ]; then
                show_available_token_urls_then_exit
            fi
            # shellcheck disable=SC2086
            p11tool $P11_TOOL_ARGS --initialize-pin "$TOKEN_URL"
            ;;
        tpm2)
            if [ -z "$TOKEN_URL" ]; then
                show_available_token_urls_then_exit
            fi
            usermod -a -G tss tedge ||:

            mkdir -p "$TPM2_PKCS11_STORE"
            chown -R tedge:tedge "$TPM2_PKCS11_STORE"

            if ! grep -q "^TPM2_PKCS11_STORE=\"$TPM2_PKCS11_STORE\"" "$TEDGE_CONFIG_DIR/plugins/tedge-p11-server.conf" 2>/dev/null; then
                cat <<EOT >> "$TEDGE_CONFIG_DIR/plugins/tedge-p11-server.conf"
# TPM specific settings
TPM2_PKCS11_STORE="$TPM2_PKCS11_STORE"
EOT
            fi

            # must be run as the tedge user
            # shellcheck disable=SC2086
            sudo -u tedge env TPM2_PKCS11_LOG_LEVEL=0 TPM2_PKCS11_STORE="$TPM2_PKCS11_STORE" GNUTLS_PIN="$GNUTLS_PIN" GNUTLS_SO_PIN="$GNUTLS_SO_PIN" p11tool $P11_TOOL_ARGS --initialize --label "$TOKEN_LABEL" "$TOKEN_URL"

            # initialize the new slot's pin and so-pin
            # shellcheck disable=SC2086
            sudo -u tedge env TPM2_PKCS11_LOG_LEVEL=0 TPM2_PKCS11_STORE="$TPM2_PKCS11_STORE" GNUTLS_PIN="$GNUTLS_PIN" GNUTLS_SO_PIN="$GNUTLS_SO_PIN" p11tool $P11_TOOL_ARGS --initialize-pin "$TEDGE_TOKEN_URL"
            ;;
        softhsm2|softhsm)
            # Note: softhsm does not require a TOKEN_URL
            usermod -a -G softhsm tedge ||:
            sudo -u tedge softhsm2-util --init-token --free --label "$TOKEN_LABEL" --pin "$GNUTLS_PIN" --so-pin "$GNUTLS_SO_PIN"
            ;;
        *)
            echo "Warning: Unknown HSM type (name=$1). Trying to initialize using standard p11tool commands" >&2
            # shellcheck disable=SC2086
            sudo -u tedge GNUTLS_PIN="$GNUTLS_PIN" GNUTLS_SO_PIN="$GNUTLS_SO_PIN" p11tool $P11_TOOL_ARGS --initialize-pin "$TOKEN_URL"
            ;;
    esac

    # Restart the existing tedge-p11-server instance so it can reload the new key (used later on)
    if command -V systemctl >/dev/null 2>&1; then
        systemctl restart tedge-p11-server.socket ||:
    fi

    echo "Creating a private key" >&2
    TEDGE_TOKEN_URL="pkcs11:token=$TOKEN_LABEL"

    CREATE_KEY_OPTIONS=
    if [ -n "$TOKEN_ID" ]; then
        CREATE_KEY_OPTIONS="$CREATE_KEY_OPTIONS --id $TOKEN_ID"
    fi

    if [ -n "$KEY_TYPE" ]; then
        CREATE_KEY_OPTIONS="$CREATE_KEY_OPTIONS --type $KEY_TYPE"
    fi

    if [ -n "$RSA_BITS" ]; then
        CREATE_KEY_OPTIONS="$CREATE_KEY_OPTIONS --bits $RSA_BITS"
    fi

    if [ -n "$ECDSA_CURVE" ]; then
        CREATE_KEY_OPTIONS="$CREATE_KEY_OPTIONS --curve $ECDSA_CURVE"
    fi

    # shellcheck disable=SC2086
    tedge cert create-key-hsm \
        $CREATE_KEY_OPTIONS \
        --label "$TOKEN_LABEL" \
        --outfile-pubkey "$PUBLIC_KEY" \
        "$TEDGE_TOKEN_URL"
}

show_available_token_urls_then_exit() {
    printf "You must provide the slot URL required for initialization. Available token urls:\n\n" >&2
    AVAILABLE_TOKENS=$(TPM2_PKCS11_LOG_LEVEL=0 p11tool --list-token-urls 2>/dev/null ||:)
    echo "$AVAILABLE_TOKENS"
    echo "" >&2
    FIRST_AVAILABLE_TOKEN=$(echo "$AVAILABLE_TOKENS" | head -n1)
    echo "Example:" >&2
    echo "  $0 --type $HSM_TYPE --token-url '$FIRST_AVAILABLE_TOKEN'" >&2
    echo "" >&2
    exit 1
}


#
# Main
#
case "$ACTION" in
    create)
        if [ -n "$TOKEN_URL" ]; then
            echo "Using Token URL: $TOKEN_URL" >&2
        fi

        systemctl enable tedge-p11-server.socket ||:

        find_pkcs11_module
        configure_tedge
        init_private_key "$HSM_TYPE"
        ;;
    *)
        echo "No action given by the user" >&2
        ;;
esac
