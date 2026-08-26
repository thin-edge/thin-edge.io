#!/bin/sh
set -e

TOKEN_URL="${TOKEN_URL:-}"

export PIN="${PIN:-123456}"
export SO_PIN="${SO_PIN:-12345678}"
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
  --type <string>           Type of HSM (using the PKCS#11 interface) to use. Available values: [tpm2, nitrokey, softhsm2, rpi_otp]
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

$0 --type tpm2 --pin $PIN --so-pin $SO_PIN
# Initialize a new slot and create a new private key pair in a TPM 2.0 module
# (the uninitialized slot is auto-discovered)

## Nitrokey

$0 --type nitrokey --pin $PIN --so-pin $SO_PIN
# Set the user PIN on a pre-initialized nitrokey (USB based HSM) and create a new key pair.
# The token is auto-selected; add --token-url '<uri>' to pick a specific token if several exist.


## SoftHSM2

$0 --type softhsm2 --pin $PIN --so-pin $SO_PIN
# Initialize a new slot and create a new private key pair using softhsm2 (for testing only)

## Raspberry Pi 4/5 (requires latest EEPROM and https://github.com/embetrix/rpifwcrypto-pkcs11 to be installed)
$0 --type rpi_otp
# Initialize the rpi-otp to create a single token (which can only be written once).

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
            PIN="$2"
            shift
            ;;
        --so-pin)
            SO_PIN="$2"
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
        echo "Removing previous 'device.cryptoki.module_path' setting. value=$VALUE" >&2
        tedge config unset device.cryptoki.module_path 2>/dev/null ||:
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
        rpi_otp)
            PKCS11_MODULE=$(find /usr/lib -name rpifwcrypto-pkcs11.so | head -n1)
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
    if [ ! -f "$PKCS11_MODULE" ]; then
        echo "ERROR: PKCS11 module does not exist. path=$PKCS11_MODULE"
        exit 1
    fi
    tedge config set mqtt.bridge.built_in true
    tedge config set device.cryptoki.mode socket
    tedge config set device.cryptoki.module_path "$PKCS11_MODULE"
    tedge config set device.cryptoki.pin "$PIN"
}

# (Re)start tedge-p11-server so it (re)loads the configured module and any HSM-specific
# environment. It must be running before `tedge hsm init` / `tedge hsm create-key`, which
# initialize the token and create the key through it via the PKCS#11 interface.
restart_p11_server() {
    if command -V systemctl >/dev/null 2>&1; then
        systemctl restart tedge-p11-server.socket ||:
    fi
}

# Set (reset) the token's user PIN using the Security Officer PIN through tedge-p11-server, i.e. an
# SO login followed by C_InitPIN. This replaces `p11tool --initialize-pin` for tokens that ship
# pre-initialized (e.g. Nitrokey/SmartCard-HSM). The token is auto-selected when it is the only
# initialized one; $TOKEN_URL selects a specific token when several are present (change-pin lists
# the available URIs if it cannot pick one).
set_user_pin_via_so() {
    if [ -n "$TOKEN_URL" ]; then
        tedge hsm change-pin --reset --new-pin "$PIN" --so-pin "$SO_PIN" "$TOKEN_URL"
    else
        tedge hsm change-pin --reset --new-pin "$PIN" --so-pin "$SO_PIN"
    fi
}

init_private_key() {
    case "$1" in
        rpi_otp)
            # NOTES: Supported on Raspberry 4 and 5, but also needs an up-to-date EEPROM
            if ! command -V rpi-fw-crypto >/dev/null 2>&1; then
                echo "ERROR: Missing 'rpi-fw-crypto' command. Please install it (see https://github.com/embetrix/rpifwcrypto-pkcs11) and try again" >&2
                exit 1
            fi
            # NOTE: tedge hsm create-key isn't supported so it needs to be manually created
            if ! rpi-fw-crypto pubkey --key-id 1 >/dev/null 2>&1; then
                echo "Initializing Raspberry Pi OTP Key" >&2
                if ! rpi-fw-crypto genkey --key-id 1 --alg ec; then
                    echo "ERROR: Failed to create a key. Try updating the Raspberry PI EEPROM using 'sudo rpi-eeprom-update -a' and try again"
                    exit 1
                fi
            fi
            restart_p11_server

            # No token initialization and no PIN: the token ships initialized and doesn't set
            # CKF_LOGIN_REQUIRED. Select the key by id - the module ignores CKA_LABEL, so a
            # label-based URI would be recorded but meaningless.
            TEDGE_TOKEN_URL="pkcs11:id=%01"
            TOKEN_LABEL="OTP Key 1"
            TOKEN_ID=
            ;;
        softhsm2|softhsm)
            # SoftHSM2 supports token initialization via the PKCS#11 interface (C_InitToken), so
            # tedge-p11-server can initialize the token directly - no softhsm2-util needed. Allow the
            # tedge user (which runs the server) to access the softhsm token store.
            # Note: softhsm does not require a TOKEN_URL.
            usermod -a -G softhsm tedge ||:
            restart_p11_server
            tedge hsm init --label "$TOKEN_LABEL" --pin "$PIN" --so-pin "$SO_PIN"
            ;;
        tpm2)
            usermod -a -G tss tedge ||:

            mkdir -p "$TPM2_PKCS11_STORE"
            chown -R tedge:tedge "$TPM2_PKCS11_STORE"

            if ! grep -q "^TPM2_PKCS11_STORE=\"$TPM2_PKCS11_STORE\"" "$TEDGE_CONFIG_DIR/plugins/tedge-p11-server.conf" 2>/dev/null; then
                cat <<EOT >> "$TEDGE_CONFIG_DIR/plugins/tedge-p11-server.conf"
# TPM specific settings
TPM2_PKCS11_STORE="$TPM2_PKCS11_STORE"
EOT
            fi

            # Restart so the server picks up TPM2_PKCS11_STORE, then let it initialize the token
            # (C_InitToken + C_InitPIN) through the PKCS#11 interface. The uninitialized slot is
            # auto-discovered, so a TOKEN_URL is not required.
            restart_p11_server
            tedge hsm init --label "$TOKEN_LABEL" --pin "$PIN" --so-pin "$SO_PIN"
            ;;
        nitrokey)
            # Nitrokey (SmartCard-HSM) tokens ship pre-initialized, so the token itself does not
            # need C_InitToken (which these tokens don't support anyway) - only the user PIN needs
            # to be set. tedge-p11-server must be running first, since it performs the PIN change
            # through the PKCS#11 interface.
            restart_p11_server
            set_user_pin_via_so
            ;;
        *)
            # Unknown HSM type: assume the token ships pre-initialized (like a Nitrokey) and just
            # set the user PIN via the Security Officer PIN through the PKCS#11 interface.
            echo "Warning: Unknown HSM type (name=$1). Assuming a pre-initialized token and setting the user PIN via the Security Officer PIN." >&2
            restart_p11_server
            set_user_pin_via_so
            ;;
    esac

    echo "Creating a private key" >&2
    if [ -z "$TEDGE_TOKEN_URL" ]; then
        TEDGE_TOKEN_URL="pkcs11:token=$TOKEN_LABEL"
    fi

    set -- --outfile-pubkey "$PUBLIC_KEY"
    if [ -n "$TOKEN_ID" ]; then set -- "$@" --id "$TOKEN_ID"; fi
    if [ -n "$KEY_TYPE" ]; then set -- "$@" --type "$KEY_TYPE"; fi
    if [ -n "$RSA_BITS" ]; then set -- "$@" --bits "$RSA_BITS"; fi
    if [ -n "$ECDSA_CURVE" ]; then set -- "$@" --curve "$ECDSA_CURVE"; fi
    if [ -n "$TOKEN_LABEL" ]; then set -- "$@" --label "$TOKEN_LABEL"; fi
    if [ -n "$TEDGE_TOKEN_URL" ]; then set -- "$@" "$TEDGE_TOKEN_URL"; fi

    tedge hsm create-key "$@"
}

#
# Main
#
case "$ACTION" in
    create)
        if [ -n "$TOKEN_URL" ]; then
            echo "Using Token URL: $TOKEN_URL" >&2
        fi

        if command -V systemctl >/dev/null 2>&1; then
            systemctl enable tedge-p11-server.socket ||:
        fi

        find_pkcs11_module
        configure_tedge
        init_private_key "$HSM_TYPE"
        ;;
    *)
        echo "No action given by the user" >&2
        ;;
esac
