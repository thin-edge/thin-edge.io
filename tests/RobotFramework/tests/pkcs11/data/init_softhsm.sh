#!/bin/sh
# legacy script which is only used in tests when create-key-hsm is not available
set -e

DEVICE_ID="${DEVICE_ID:-}"
IS_SELF_SIGNED=0
export GNUTLS_PIN="${GNUTLS_PIN:-123456}"
export GNUTLS_SO_PIN="${GNUTLS_SO_PIN:-123456}"
export TOKEN_LABEL="${TOKEN_LABEL:-tedge}"
PKCS_URI=

#
# Parse arguments
#
while [ $# -gt 0 ]; do
    case "$1" in
        --self-signed)
            IS_SELF_SIGNED=1
            ;;
        --label)
            TOKEN_LABEL="$2"
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
        --device-id)
            DEVICE_ID="$2"
            shift
            ;;
    esac
    shift
done

get_token() {
    p11tool --list-tokens | grep "token=$TOKEN_LABEL" | awk '{ print $2 }' | head -n1
}

get_key() {
    p11tool --login --list-all "$PKCS_URI" | grep type=private | awk '{ print $2 }'
}

#
# Get/Init slot
#
PKCS_URI=$(get_token)
if [ -z "$(get_token)" ]; then
    echo "Initializing softhsm2 token" >&2
    softhsm2-util --init-token --free --label "$TOKEN_LABEL" --pin "$GNUTLS_PIN" --so-pin "$GNUTLS_SO_PIN"
    PKCS_URI=$(get_token)
fi
echo "Using token URI: $PKCS_URI" >&2


#
# Get/Create key
#
KEY=$(get_key)
if [ -z "$KEY" ]; then
    mkdir -p /etc/tedge/hsm
    p11tool --login --generate-privkey ECDSA --curve=secp256r1 --label "$TOKEN_LABEL" --outfile "/etc/tedge/hsm/${TOKEN_LABEL}.pub" "$PKCS_URI"
    KEY=$(get_key)
fi


#
# Get/Create CSR template
#
CSR_TEMPLATE=/etc/tedge/hsm/cert.template
if [ ! -f "$CSR_TEMPLATE" ]; then
    if [ -z "${DEVICE_ID:-}" ]; then
        DEVICE_ID=$(tedge-identity 2>/dev/null)
    fi

    # If it is self-signed, then Cumulocity requires the ca property
    # to be added, otherwise certificate will be rejected by Cumulocity
    # when trying to upload it
    IS_CA=""
    if [ "$IS_SELF_SIGNED" ]; then
        IS_CA="ca"
    fi

    cat <<EOT > "$CSR_TEMPLATE"
organization = "Thin Edge"
unit = "Test Device"
state = "QLD"
country = AU
cn = "$DEVICE_ID"
$IS_CA
EOT
fi

#
# Create CSR (to be signed externally) or create a self-signed certificate
#
if [ "$IS_SELF_SIGNED" = 0 ]; then
    #
    # Create CSR
    #
    CSR_PATH=$(tedge config get device.csr_path)
    certtool --generate-request --template "$CSR_TEMPLATE" --load-privkey "$KEY" --outfile "$CSR_PATH"
    echo "Created csr: $CSR_PATH" >&2
else
    # Optional: Self sign the Certificate
    echo "Creating self-signed certificate" >&2
    CERT_PATH=$(tedge config get device.cert_path)
    certtool --generate-self-signed --template "$CSR_TEMPLATE" --load-privkey "$KEY" --outfile "$CERT_PATH"
fi
