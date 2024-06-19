#!/usr/bin/env bash

# This script generates the certificates required for the "unit"
# tests in axum_tls using openssl

days=365000
args=("-days" "$days" "-noenc" \
        -subj "/CN=localhost" \
        -addext "subjectAltName=DNS:localhost,DNS:*.localhost,IP:127.0.0.1" \
        -addext "basicConstraints=critical,CA:false")

set -eux

openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -keyout ec.pkcs8.key -out ec.crt "${args[@]}"
openssl req -x509 -newkey rsa -keyout rsa.pkcs8.key -out rsa.crt "${args[@]}"
openssl req -x509 -newkey ed25519 -keyout ed25519.key -out ed25519.crt "${args[@]}"

openssl ec -in ec.pkcs8.key -out ec.key
openssl pkey -in rsa.pkcs8.key -out rsa.pkcs1.key -traditional