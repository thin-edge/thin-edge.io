#!/bin/sh

set -e

openssl req \
    -new \
    -x509 \
    -days 7 \
    -extensions v3_ca \
    -nodes \
    -subj "/C=US/ST=Denial/L=Springfield/O=Dis/CN=ca" \
    -keyout ca.key \
    -out ca.crt

openssl genrsa -out server.key 2048

openssl req -out server.csr -key server.key -new \
    -subj "/C=US/ST=Denial/L=Springfield/O=Dis/CN=$(hostname)"

cat > v3.ext << EOF
authorityKeyIdentifier=keyid
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyAgreement
subjectAltName=DNS:$(hostname),DNS:localhost
EOF

openssl x509 -req \
    -in server.csr \
    -CA ca.crt \
    -CAkey ca.key \
    -extfile v3.ext \
    -CAcreateserial \
    -out server.crt \
    -days 7

openssl genrsa -out client.key 2048

openssl req -out client.csr \
    -key client.key \
    -subj "/C=US/ST=Denial/L=Springfield/O=Dis/CN=client1" \
    -new

openssl x509 -req \
    -in client.csr \
    -CA ca.crt \
    -CAkey ca.key \
    -CAcreateserial \
    -out client.crt \
    -days 7

sudo mv ca* /etc/mosquitto/ca_certificates
sudo mv server* /etc/mosquitto/ca_certificates

sudo chown -R mosquitto:mosquitto /etc/mosquitto/ca_certificates
