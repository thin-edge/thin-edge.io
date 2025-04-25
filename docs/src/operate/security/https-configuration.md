---
title: HTTPS Configuration
tags: [Operate, Security, HTTP]
description: Setting up HTTPS for secure local communication
---

%%te%% provides two services over HTTP:
- The [File Transfer Service](../../references/file-transfer-service.md) is used by the mappers and the child devices to transfer files locally.
- The [Cumulocity Proxy](../../references/cumulocity-proxy.md) acts as a local proxy to the Cumulocity REST API.

Three levels of security are supported:

1. HTTP without any client authentication (default)
2. HTTPS with server authentication
3. HTTPS with server and client authentication


## Default HTTP Setting

### File Transfer Service

The **tedge-agent** running on the main device acts as a local HTTP server
which is the mappers and the child devices to transfer files locally.
Any local process can PUT and GET files there:

```sh
echo "Hello thin-edge.io" >/tmp/foo.txt
curl -X PUT -F 'file=@/tmp/foo.txt' http://localhost:8000/te/v1/files/foo.txt
curl http://localhost:8000/te/v1/files/foo.txt 
```

### Cumulocity Proxy

When a device is successfully connected to Cumulocity,
**tedge-mapper** acts as a proxy to the Cumulocity REST API.
For instance, the following lists the managed objects related to the device:

```sh
curl http://localhost:8001/c8y/inventory/managedObjects 
```

:::note
The connection from the Cumulocity Proxy to Cumulocity is always established over HTTPS,
whatever the settings for local connections.

The identity of the Cumulocity end-point is authenticated using the
root certificates configured with `tedge config get c8y.proxy.ca_path` and the device itself
is authenticated using JWT tokens retrieved from Cumulocity via MQTT.
:::

## Open HTTP services on the local network

Per default, the File Transfer Service and Cumulocity Proxy are only available on `localhost`,
the loopback network interface of the main device.
To open these services to child devices, their *bind* addresses must be set explicitly.

### File Transfer Service

On the __main device__, two `tedge config` settings define the binding address and port of the File Transfer Service:

- `http.bind.address` The bind address of the File Transfer Service HTTP server
- `http.bind.port`  The port number of the File Transfer Service HTTP server

On a __child device__,  two `tedge config` settings define how to connect the File Transfer Service:

- `http.client.host` The address or hostname of the main device where the File Transfer Service HTTP server is running
- `http.client.port`  The port number on main device on which the File Transfer Service HTTP server is running

For instance, assuming a main device named `rpi4-dca632efb150` with `192.168.1.6` as IP address,
one has first to configure and restart *tedge-agent* of the main device:

```sh title="main device"
sudo tedge config set http.bind.address 192.168.1.6
sudo systemctl restart tedge-agent
```

and then, on the child device, *tedge-agent* needs to be configured to use the File Transfer Service of the main device: 

```sh title="child device"
sudo tedge config set http.client.host rpi4-dca632efb150
sudo systemctl restart tedge-agent
```

Any process running on a child device can also use the File Transfer Service:

```sh title="child device"
echo "Hello thin-edge.io" >/tmp/foo.txt
curl -X PUT -F 'file=@/tmp/foo.txt' http://rpi4-dca632efb150:8000/te/v1/files/foo.txt
curl http://rpi4-dca632efb150:8000/te/v1/files/foo.txt 
```

:::note
The firewall of the main device has also to be configured
to accept incoming requests on the port used by the File Transfer Service (`8000` per default)
:::

### Cumulocity Proxy

As for the File Transfer Service, the Cumulocity Proxy is configured using four settings,
two to be used by the Cumulocity **tedge-mapper**, and two used by its clients
(mainly the **tedge-agent**, running on the main device or a child devices).

- `c8y.proxy.bind.address`  The IP address local Cumulocity HTTP proxy binds to
- `c8y.proxy.bind.port`  The port local Cumulocity HTTP proxy binds to
- `c8y.proxy.client.host`  The address of the host on which the local Cumulocity HTTP Proxy is running
- `c8y.proxy.client.port`  The port number on the remote host on which the local Cumulocity HTTP Proxy is running

For instance, assuming a main device named `rpi4-dca632efb150` with `192.168.1.6` as IP address,
one has first to configure and restart *tedge-mapper-c8y*:

```sh title="device running tedge-mapper-c8y"
sudo tedge config set c8y.proxy.bind.address 192.168.1.6
sudo systemctl restart tedge-mapper-c8y
```

and then, on all the devices, *tedge-agent* needs to be configured to connect the Cumulocity HTTP Proxy:

```sh title="main and child device"
sudo tedge config set c8y.proxy.client.host rpi4-dca632efb150
sudo systemctl restart tedge-agent
```

Any process running on a child device can also use the Cumulocity HTTP Proxy:

```sh title="main and child device"
curl http://rpi4-dca632efb150:8001/c8y/inventory/managedObjects 
```

:::note
The firewall of the box running the Cumulocity mapper has also to be configured
to accept incoming requests on the port used by the Cumulocity HTTP Proxy (`8001` per default)
:::

## Enable HTTPS

The next step is to enable HTTPS, the clients authenticating the servers.

For that, one needs two certificates:
- one for the main **tedge-agent** running the File Transfer Service
- another for the Cumulocity **tedge-mapper** running the Cumulocity HTTP Proxy.

These two certificates have also to be trusted by the clients, i.e. the child devices. 
Hence, the signing certificate will have to be added to the list of trusted root certificates on each child device.

### Generating Certificates

%%te%% currently provides no specific tool to generate and deploy certificates over child devices.

You can use the following script to generate all the required certificates.

First, one needs a signing key that will be used to sign the certificates of the **tedge-agent** and **tedge-mapper**:

```sh
DEVICE=$(tedge config get device.id)

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
```

This signing certificate has to be trusted by the main device as well as all the child devices:

```sh title="on the main as well as the child devices"
cp tedge-local-ca.crt /usr/local/share/ca-certificates
sudo update-ca-certificates
```

One can then proceed with a certificate for the **tedge-agent** of the main device:

```sh
## main agent private key
openssl genrsa -out main-agent.key 2048

## main agent certificate signing request
openssl req -out main-agent.csr \
    -key main-agent.key \
    -subj "/O=thin-edge/OU=$DEVICE/SN=main-agent/CN=localhost" \
    -new

## signing the main agent certificate
cat > v3.ext << EOF
authorityKeyIdentifier=keyid
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyAgreement
extendedKeyUsage = serverAuth, clientAuth
subjectAltName=DNS:$(hostname),DNS:localhost
EOF

openssl x509 -req \
    -in main-agent.csr \
    -CA tedge-local-ca.crt \
    -CAkey tedge-local-ca.key \
    -extfile v3.ext \
    -CAcreateserial \
    -out main-agent.crt \
    -days 100
```

:::note
Note that the hostname used as `subjectAltName` must be the hostname used by the clients to client to connect **tedge-agent**.
If you want to connect **tedge-agent** using its IP address, say `192.168.1.6`,
then the `subjectAltName` must be set to `subjectAltName=IP:192.168.1.6`.
::: 

And another one for the **tedge-mapper**:

```sh
## mapper private key
openssl genrsa -out c8y-mapper.key 2048

## mapper certificate signing request
openssl req -out c8y-mapper.csr -key c8y-mapper.key \
    -subj "/O=thin-edge/OU=$DEVICE/SN=c8y-mapper/CN=localhost" \
    -new

## signing the mapper certificate
cat > v3.ext << EOF
authorityKeyIdentifier=keyid
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyAgreement
extendedKeyUsage = serverAuth, clientAuth
subjectAltName=DNS:$(hostname),DNS:localhost
EOF

openssl x509 -req \
    -in c8y-mapper.csr \
    -CA tedge-local-ca.crt \
    -CAkey tedge-local-ca.key \
    -extfile v3.ext \
    -CAcreateserial \
    -out c8y-mapper.crt \
    -days 100
```

### File Transfer Service

Two `tedge config` settings enable HTTPS for the File Transfer Service of the main device.

- `http.cert_path`  The file that will be used as the server certificate for the File Transfer Service. 
- `http.key_path`  The file that will be used as the server private key for the File Transfer Service.

Assuming the main device certificate is stored in `/etc/tedge/device-local-certs`,
one simply has to set these settings and to restart the **tedge-agent** to enable HTTPS:

```sh title="main device"
sudo tedge config set http.cert_path /etc/tedge/device-local-certs/main-agent.crt
sudo tedge config set http.key_path /etc/tedge/device-local-certs/main-agent.key
sudo systemctl restart tedge-agent
```

Nothing has to be done on the child devices (except trusting the signing certificate).
And file transfer is now available over HTTPS:

```sh title="child device"
echo "Hello thin-edge.io" >/tmp/foo.txt
curl -X PUT -F 'file=@/tmp/foo.txt' https://rpi4-dca632efb150:8000/te/v1/files/foo.txt
curl https://rpi4-dca632efb150:8000/te/v1/files/foo.txt 
```

### Cumulocity Proxy

Two `tedge config` settings enable HTTPS for the Cumulocity Proxy.

- `c8y.proxy.cert_path`  The file that will be used as the server certificate for the Cumulocity proxy. 
- `c8y.proxy.key_path`  The file that will be used as the server private key for the Cumulocity proxy. 

Assuming the main device certificate is stored in `/etc/tedge/device-local-certs`,
one simply has to set these settings and to restart the **tedge-mapper** to enable HTTPS:

```sh title="on the box running c8y mapper"
tedge config set c8y.proxy.cert_path /etc/tedge/device-local-certs/c8y-mapper.crt
tedge config set c8y.proxy.key_path /etc/tedge/device-local-certs/c8y-mapper.key
sudo systemctl restart tedge-mapper-c8y
```

Nothing has to be done on the child devices (except trusting the signing certificate).
And the Cumulocity proxy is now available over HTTPS:

```sh title="child device"
echo "Hello thin-edge.io" >/tmp/foo.txt
curl -X PUT -F 'file=@/tmp/foo.txt' https://rpi4-dca632efb150:8000/te/v1/files/foo.txt
curl https://rpi4-dca632efb150:8000/te/v1/files/foo.txt 
```

```sh title="main and child device"
curl https://rpi4-dca632efb150:8001/c8y/inventory/managedObjects 
```

## Enable HTTPS client authentication

The final step is to enforce certificate-based authentication to the clients connecting 
the File Transfer Service and the Cumulocity Proxy.

### Generating Certificates

Each child device must be given a certificate.
The simpler is to use the same signing key as for the servers. This is not mandatory though.

```sh title="on each child device"
## child device private key
openssl genrsa -out tedge-client.key 2048

## child device certificate signing request
openssl req -out tedge-client.csr \
    -key tedge-client.key \
    -subj "/O=thin-edge/OU=$DEVICE/SN=child/CN=tedge-client" \
    -new
```

The certificate signing request (CSR) has to be signed on the box where the signing key is stored.

```sh title="on the laptop owning the signing key"
## signing the child device certificate
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
```

The resulting certificate can then be copied and used on the child device.

### File Transfer Service

A single `tedge config` setting enables client authentication (once HTTPS is already enabled).

- `http.ca_path`  Path to a directory containing the PEM encoded CA certificates that are trusted
  when checking incoming client certificates for the File Transfer Service.

Assuming the signing certificate used for the child device has been properly added to `/etc/ssl/certs`,
one simply has to set `http.ca_path` and to restart the **tedge-agent** to enforce client authentication:

```sh title="main device"
sudo tedge config set http.ca_path /etc/ssl/certs
sudo systemctl restart tedge-agent
```

Clients have then to authenticate themselves using a certificate trusted by the main **tedge-agent**:

```sh title="child device"
echo "Hello thin-edge.io" >/tmp/foo.txt
curl --cert /etc/tedge/device-local-certs/tedge-client.crt \
     --key /etc/tedge/device-local-certs/tedge-client.key \
     -X PUT -F 'file=@/tmp/foo.txt' \
     https://rpi4-dca632efb150:8000/te/v1/files/foo.txt
curl --cert /etc/tedge/device-local-certs/tedge-client.crt \
     --key /etc/tedge/device-local-certs/tedge-client.key \
     https://rpi4-dca632efb150:8000/te/v1/files/foo.txt 
```

Notably, **tedge-agent** must be updated on each child device,
using the following `tedge config` settings:

- `http.client.auth.cert_file`  Path to the certificate which is used by the agent when connecting to external services
- `http.client.auth.key_file`  Path to the private key which is used by the agent when connecting to external services

```sh title="child device"
sudo tedge config set http.client.auth.cert_file /etc/tedge/device-local-certs/tedge-client.crt
sudo tedge config set http.client.auth.key_file /etc/tedge/device-local-certs/tedge-client.key
sudo systemctl restart tedge-agent
```

### Cumulocity Proxy

A single `tedge config` setting enables client authentication (once HTTPS is already enabled).

- `c8y.proxy.ca_path`  Path to a file containing the PEM encoded CA certificates that are trusted
  when checking incoming client certificates for the Cumulocity Proxy.

Assuming the signing certificate used for the child device has been properly added to `/etc/ssl/certs`,
one simply has to set `c8y.proxy.ca_path` and to restart the **tedge-mapper** to enforce client authentication:

```sh title="on the box running tedge-mapper"
sudo tedge config set c8y.proxy.ca_path /etc/ssl/certs
sudo systemctl restart tedge-mapper-c8y
```

Clients have then to authenticate themselves using a certificate trusted by the main **tedge-mapper**:

```sh title="child device"
curl --cert /etc/tedge/device-local-certs/tedge-client.crt \
     --key /etc/tedge/device-local-certs/tedge-client.key \
     https://rpi4-dca632efb150:8001/c8y/inventory/managedObjects 
```

Notably, **tedge-agent** must be updated on the main device and all the child devices,
using the following `tedge config` settings:

- `http.client.auth.cert_file`  Path to the certificate which is used by the agent when connecting to external services
- `http.client.auth.key_file`  Path to the private key which is used by the agent when connecting to external services

```sh title="main device as well as child devices"
sudo tedge config set http.client.auth.cert_file /etc/tedge/device-local-certs/tedge-client.crt
sudo tedge config set http.client.auth.key_file /etc/tedge/device-local-certs/tedge-client.key
sudo systemctl restart tedge-agent
```
