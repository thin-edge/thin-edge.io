#!/usr/bin/env bash
DEVICE=$1
CERT_PATH=$2
KEY_PATH=$3

if [ -z "$DEVICE" -o -z "$CERT_PATH" -o -z "$KEY_PATH" -o "$#" -ne 3 ]
then
    echo "usage: $0 IDENTIFIER CERT-PATH KEY-PATH"
    echo
    echo "Generates a self signed certificate"
    echo "using the given IDENTIFIER as common name."
    echo
    echo "The certificate is stored in CERT-PATH"
    echo "The private key is stored in KEY-PATH"
    exit 1
fi

if [ -f "$CERT_PATH" ]
then
    echo "[ERROR] The file $CERT_PATH already exists"
    exit 1
fi

if [ -f "$KEY_PATH" ]
then
    echo "[ERROR] The file $KEY_PATH already exists"
    exit 1
fi

# see https://www.mkssoftware.com/docs/man1/openssl_req.1.asp

CONFIG="
[ req ]
default_bits            = 2048
distinguished_name = dist_name
x509_extensions = v3_ca
output_password = nopass
prompt = no

[ dist_name ]
commonName = $DEVICE
organizationName = 'Thin Edge'
organizationalUnitName	= 'Test Device'

[ v3_ca ]
basicConstraints = CA:true
"

openssl req -config <(echo "$CONFIG") -new -nodes -x509 -days 365 -extensions v3_ca -keyout $KEY_PATH -out $CERT_PATH

if [ -f $CERT_PATH ]
then
    echo "[OK] The device certificate is stored in $CERT_PATH"
else
    echo "[ERROR] No device certificate has been created"
    exit 1
fi
