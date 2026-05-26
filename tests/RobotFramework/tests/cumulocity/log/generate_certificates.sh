#!/bin/sh

set -e

DEVICE=$(tedge config get device.id)
C8Y_PROXY_COMMON_NAME=$(tedge config get c8y.proxy.client.host)
FTS_COMMON_NAME=$(tedge config get http.client.host)

## Signing certificate
openssl req \
    -new \
    -x509 \
    -days 100 \
    -extensions v3_ca \
    -nodes \
    -subj "/O=thin-edge/OU=$DEVICE/CN=tedge-ca" \
    -keyout tedge-local-ca.key \
    -out tedge-local-ca.crt

## c8y mapper certificate

openssl genrsa -out c8y-mapper.key 2048

openssl req -out c8y-mapper.csr -key c8y-mapper.key \
    -subj "/O=thin-edge/OU=$DEVICE/SN=c8y-mapper/CN=$C8Y_PROXY_COMMON_NAME" \
    -new

cat > v3.c8yproxy.ext << EOF
authorityKeyIdentifier=keyid
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyAgreement
extendedKeyUsage = serverAuth, clientAuth
subjectAltName=DNS:localhost,IP:$C8Y_PROXY_COMMON_NAME,IP:127.0.0.1
EOF

openssl x509 -req \
    -in c8y-mapper.csr \
    -CA tedge-local-ca.crt \
    -CAkey tedge-local-ca.key \
    -extfile v3.c8yproxy.ext \
    -CAcreateserial \
    -out c8y-mapper.crt \
    -days 100

## main agent certificate

cat > v3.agent.ext << EOF
authorityKeyIdentifier=keyid
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyAgreement
extendedKeyUsage = serverAuth, clientAuth
subjectAltName=DNS:localhost,IP:$FTS_COMMON_NAME,IP:127.0.0.1
EOF

openssl genrsa -out main-agent.key 2048

openssl req -out main-agent.csr \
    -key main-agent.key \
    -subj "/O=thin-edge/OU=$DEVICE/SN=main-agent/CN=$FTS_COMMON_NAME" \
    -new

openssl x509 -req \
    -in main-agent.csr \
    -CA tedge-local-ca.crt \
    -CAkey tedge-local-ca.key \
    -extfile v3.agent.ext \
    -CAcreateserial \
    -out main-agent.crt \
    -days 100

## client certificate

openssl genrsa -out tedge-client.key 2048

openssl req -out tedge-client.csr \
    -key tedge-client.key \
    -subj "/O=thin-edge/OU=$DEVICE/SN=child/CN=tedge-client" \
    -new

cat > client-v3.ext << EOF
basicConstraints=CA:FALSE
extendedKeyUsage = clientAuth
EOF

openssl x509 -req \
    -in tedge-client.csr \
    -CA tedge-local-ca.crt \
    -CAkey tedge-local-ca.key \
    -extfile client-v3.ext \
    -CAcreateserial \
    -out tedge-client.crt \
    -days 100

## Settings

#cat tedge-local-ca.crt >> main-agent.crt

mkdir -p /etc/tedge/device-local-certs/roots

mv tedge-local-ca.* /etc/tedge/device-local-certs/roots
mv c8y-mapper.* /etc/tedge/device-local-certs
mv main-agent.* /etc/tedge/device-local-certs
mv tedge-client.* /etc/tedge/device-local-certs

sudo cp /etc/tedge/device-local-certs/roots/tedge-local-ca.crt /usr/local/share/ca-certificates
sudo update-ca-certificates

### c8y mapper (serving c8y-proxy, file-transfer client)
#tedge config set c8y.proxy.ca_path /etc/tedge/device-local-certs/roots
#tedge config set c8y.proxy.cert_path /etc/tedge/device-local-certs/c8y-mapper.crt
#tedge config set c8y.proxy.key_path /etc/tedge/device-local-certs/c8y-mapper.key

### main agent (serving file-transfer, c8y-proxy client)
tedge config set http.client.auth.cert_file /etc/tedge/device-local-certs/main-agent.crt
tedge config set http.client.auth.key_file /etc/tedge/device-local-certs/main-agent.key
tedge config set http.cert_path /etc/tedge/device-local-certs/main-agent.crt
tedge config set http.key_path /etc/tedge/device-local-certs/main-agent.key
#tedge config set http.ca_path /etc/tedge/device-local-certs/roots

chown -R tedge:tedge /etc/tedge/device-local-certs
