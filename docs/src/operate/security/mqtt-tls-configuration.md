---
title: MQTT TLS Configuration
tags: [Operate, Security, MQTT]
description: Setting up TLS for secure local MQTT communication
---

%%te%% supports certificate-based authentication when communicating with
an MQTT broker. Three levels of security are supported:

1. No authentication (default)
2. Server authentication
3. Server + client authentication

## MQTT Configuration

The `tedge config` command provides MQTT specific settings
to open to the child devices the MQTT broker running on the gateway device.

|Property|Description|
|--------|-----------|
|`mqtt.external.bind.port`|Mqtt broker port, which is used by the external mqtt clients to publish or subscribe. Example: 8883|
|`mqtt.external.bind.address`|IP address / hostname, which the mqtt broker limits incoming connections on. Example: 0.0.0.0|
|`mqtt.external.bind.interface`|Name of network interface, which the mqtt broker limits incoming connections on. Example: wlan0|
|`mqtt.external.ca_path`|Path to a file containing the PEM encoded CA certificates that are trusted when checking incoming client certificates. Example: /etc/ssl/certs|
|`mqtt.external.cert_file`|Path to the certificate file, which is used by external MQTT listener. Example: /etc/tedge/server-certs/tedge-certificate.pem|
|`mqtt.external.key_file`|Path to the private key file, which is used by external MQTT listener. Example: /etc/tedge/server-certs/tedge-private-key.pem|

:::note
If none of these options is set, then no external listener is set.
If one of these options is set, then default values are inferred by the MQTT server (Mosquitto).
For instance, the port defaults to 1883 for a non-TLS listener, and to 8883 for a TLS listener.
:::

These settings can be considered in 2 groups, listener configuration and TLS configuration.

### Configure basic listener

To configure basic listener you should provide port and/or bind address which will use default interface.
To change the default interface you can use mqtt.external.bind.interface configuration option.

To set them you can use `tedge config` as follows:

```sh
sudo tedge config set mqtt.external.bind.port 8883
```

To allow connections from all IP addresses on the interface:

```sh
sudo tedge config set mqtt.external.bind.address "0.0.0.0"
```

### Configure TLS on the listener

To configure the external listener with TLS additional settings are available:

* `mqtt.external.ca_path`
* `mqtt.external.cert_file`
* `mqtt.external.key_file`

To enable MQTT over TLS, a server side certificate must be configured using the 2 following settings:

```sh
sudo tedge config set mqtt.external.cert_file /etc/tedge/server-certs/tedge-certificate.pem
sudo tedge config set mqtt.external.key_file /etc/tedge/server-certs/tedge-private-key.pem
```

To fully enable TLS authentication clients, client side certificate validation can be enabled:

```sh
sudo tedge config set mqtt.external.ca_path /etc/ssl/certs
```

## Server authentication

Enabling server authentication causes %%te%% MQTT clients to require a
valid certificate from a broker when connecting. The broker certificate is valid
when it is signed by a CA that the clients trust.

To enable server authentication, perform the following:

### Step 1: Configure server authenticated listener in Mosquitto broker

Create a file in `/etc/mosquitto/conf.d/` with the following content and restart
mosquitto service:

```sh
listener 8883
certfile PATH_TO_SERVER_CERTIFICATE
keyfile  PATH_TO_SERVER_PRIVATE_KEY
```

- `listener 8883`: defines a new listener on port 8883
- `certfile`: points to a certificate that will be used by the broker to
  authenticate itself to connecting clients
- `keyfile`: points to a private key of the specified certificate, necessary for
  encrypted communication

Be sure that mosquitto can read both files, especially the private key, which
can only be read by the owner. Set `mosquitto:mosquitto` as an owner of these
files. If you're unsure where to place them, `/etc/mosquitto/ca_certificates`
is the directory intended for them, although you can use other paths if
necessary.

The certificates used need to be X.509 v3 certificates with a `subjectAltName`
section containing the hostname that the broker is running on.

### Step 2: Configure thin-edge.io to connect to the new listener

Execute the following commands:

```sh
sudo tedge config set mqtt.client.port 8883
sudo tedge config set mqtt.client.cafile PATH_TO_CA_CERTIFICATE

# optional
sudo tedge config set mqtt.client.cadir PATH_TO_CA_CERTIFICATE_DIRECTORY
```

`mqtt.client.cafile` and `mqtt.client.cadir` options point to trusted CA
certificate(s) used to verify the broker. If either is used, server
authentication is enabled.

### Step 3: Restart services

Now you will need to manually restart all the affected services so that they can
pick up the configuration change.

<!-- TODO: Provide example of which services need to be restarted -->

## Server + client authentication

Additionally, the server can require connecting clients to present a valid
certificate. These client certificates need to be signed by a CA that the server
trusts. CA used to sign a server certificate and CA signing client certificates
do not have to be the same.

### Step 1: Configure server + client authenticated listener in Mosquitto broker

Change the content of the conf file defined previously to the following and
restart mosquitto service:

```conf
listener 8883
allow_anonymous false
require_certificate true
cafile   PATH_TO_SERVER_CERTIFICATE
certfile PATH_TO_SERVER_PRIVATE_KEY
keyfile  PATH_TO_CLIENT_CA_CERTIFICATE
```

- `allow_anonymous` disables anonymous access to the listener; connecting
  clients will need to authenticate themselves
- `require_certificate` requires clients to provide a certificate as a means of
  authentication

### Step 2: Configure thin-edge.io to use a client certificate and private key

```sh
sudo tedge config set mqtt.client.auth.cert_file PATH_TO_CLIENT_CERTIFICATE
sudo tedge config set mqtt.client.auth.key_file PATH_TO_CLIENT_PRIVATE_KEY
```

Both `certfile` and `keyfile` are required to enable client authentication.
Setting only one of them will result in an error about the second one not being
set.

As with the server private key, set `tedge:tedge` as the owner of the
certificate and the private key, so that the private key can be read by
%%te%% components.

### Step 3: Restart services

Now you will need to manually restart all the affected services so that they can
pick up the configuration change.

## Generating certificates

You can use the following script to generate all required certificates:

```sh
openssl req \
    -new \
    -x509 \
    -days 365 \
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
subjectAltName=DNS:$(hostname), DNS:localhost
EOF

openssl x509 -req \
    -in server.csr \
    -CA ca.crt \
    -CAkey ca.key \
    -extfile v3.ext \
    -CAcreateserial \
    -out server.crt \
    -days 365

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
    -days 365
```

## Next steps

- For more options to customize behaviour of mosquitto broker, see
  [mosquitto.conf man page](https://mosquitto.org/man/mosquitto-conf-5.html)
