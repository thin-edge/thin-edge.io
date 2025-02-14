---
title: 🚧 Hardware Security Module (HSM) support
tags: [Reference, Security]
description: Current state of HSM support in %%te%%.
draft: true
---

# Hardware Security Module (HSM) support

This document describes the state of HSM support of %%te%%.

:::note
The HSM support in %%te%% is still experimental, so it's gated behind `cryptoki` build feature
switch. Make sure %%te%% is built with `--features cryptoki`.
:::

Currently we support PKCS#11(aka cryptoki) capable modules for MQTT client authentication between
the device and the cloud.

Environment-related prerequisites:

- a PKCS#11-capable device (e.g. Yubikey*)
- a way to make the module accessible to tedge user and tedge processes
- C runtime capable of loading dynamic modules (e.g. glibc or dynamically linked musl work, but
  statically linked musl doesn't work)

On the %%te%% side, we use the private key on the module to sign necessary messages to establish a
TLS1.3 connection. The client certificate is still read from the file. It is up to the user to
ensure the private key in the HSM corresponds to the used public certificate.

For now, the module is only used for the TLS MQTT connection between the device and C8y cloud.
Additionally, the built-in bridge has to be used.

## Configuration

This feature has the following related configuration options:

```
     device.cryptoki.enable  Use a Hardware Security Module for authenticating the MQTT connection with the cloud.  When set to true, `key_path` option is ignored as PKCS#11 module is used for signing.
                             Examples: true, false
device.cryptoki.module_path  A path to the PKCS#11 module used for interaction with the HSM.
                             Example: /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so
        device.cryptoki.pin  Pin value for logging into the HSM. 
                             Example: 123456
     device.cryptoki.serial  A serial number of a Personal Identity Verification (PIV) device to be used.  Necessary if two or more modules are connected. 
                             Example: 123456789
```

## Setup guide

The following guide shows how to connect to Cumulocity using a PIV-capable Yubikey. The guide uses a
Yubikey 5C NFC.

### Setup the Yubikey

1. Make sure the Yubikey is connected.
1. Install the [ykman CLI](https://docs.yubico.com/software/yubikey/tools/ykman/).
2. Create a private key on the device. In the guide we'll import the private key, but you can also
   create one directly on the Yubikey to ensure its contents are never read by the device.

```sh
ykman piv keys import 9a $(tedge config get device.key_path)
```

4. Create a X.509 certificate on the device. This certificate won't actually be used by %%te%% as
   using a certificate from the device is currently unsupported and the certificate will still be
   read from the filesystem. However, the Yubikey won't expose the key without the certificate, so
   we still have to create it.

```sh
ykman piv certificates import 9a $(tedge config get device.cert_path)
```

### p11-kit setup

Next, we need to make sure the Yubikey is accessible as a cryptoki token by %%te%%. This can be
tricky because of different permissions or if %%te%% is inside of a container. To work around this,
we can run `p11-kit-server` on the host, which creates a UNIX socket, and have %%te%% use
`p11-kit-client` as a module to connect to the socket and access the cryptographic token.

> Additional reference: https://github.com/reubenmiller/hsm-research/blob/main/docs/CONTAINERS.md

5. Install `p11-kit`

```sh
apt-get install -y p11-kit gnutls-bin
```

6. Ensure Yubikey token is visible by `p11-kit`

```sh
p11tool list-modules
```

```
...
module: opensc-pkcs11
    path: /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so
    uri: pkcs11:library-description=OpenSC%20smartcard%20framework;library-manufacturer=OpenSC%20Project
    library-description: OpenSC smartcard framework
    library-manufacturer: OpenSC Project
    library-version: 0.25
    token: marcel-hsm-device
        uri: pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II;serial=27bef941ab9ae36e;token=marcel-hsm-device
        manufacturer: piv_II
        model: PKCS#15 emulated
        serial-number: 27bef941ab9ae36e
        flags:
              rng
              login-required
              user-pin-initialized
              token-initialized
...
```

7. Create systemd files:

Create the following systemd files

file: /etc/systemd/system/p11-kit-server.socket

```toml
[Unit]
Description=p11-kit server

[Socket]
Priority=6
Backlog=5
ListenStream=%t/p11-kit/pkcs11
SocketMode=0600

[Install]
WantedBy=sockets.target
```

file: /etc/systemd/system/p11-kit-server.service

```toml
[Unit]
Description=p11-kit server
Documentation=man:p11-kit(8)

Requires=p11-kit-server.socket

[Service]
Type=simple
StandardError=journal
ExecStart=/usr/bin/p11-kit server -f -u tedge -g tedge -n %t/p11-kit/pkcs11 pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II
# Or use a more exact filter
#ExecStart=/usr/bin/p11-kit server -f -u tedge -g tedge -n %t/p11-kit/pkcs11 pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II;serial=27bef941ab9ae36e;token=rpi5-d83addab8e9f
Restart=on-failure

[Install]
Also=p11-kit-server.socket
WantedBy=default.target
```

8. Configure the opensc-pkcs11 module for p11-kit server and client

```sh
echo "module: /usr/lib/x86_64-linux-gnu/pkcs11/opensc-pkcs11.so" > /usr/share/p11-kit/modules/opensc-pkcs11.module
echo "module: /usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so" > /etc/pkcs11/modules/p11-kit-client.module
```

9. Reload the systemd

systemctl daemon-reload
systemctl enable p11-kit-server.service
systemctl start p11-kit-server.service

10. Check if the pk11 server is reachable (from the host) and if the token is reachable via the
    p11-kit-client provider.


```sh
export P11_KIT_SERVER_ADDRESS=unix:path=/run/p11-kit/pkcs11
p11tool --provider p11-kit-client.so --list-tokens
```

11. Mount the server socket parent directory using docker. Here we're start a new container with the
    mount:

```sh
docker run -it --rm -v /run/p11-kit:/run/p11-kit -e P11_KIT_SERVER_ADDRESS=unix:path=/run/p11-kit/pkcs11 debian:12
```

12. Test if token is reachable from the container

```sh
# Install the dependencies in the container

apt-get update
apt-get install -y sudo openssl libengine-pkcs11-openssl gnutls-bin opensc
mkdir -p /etc/pkcs11/modules
echo "module: /usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so" > /etc/pkcs11/modules/p11-kit-client.module

# Test connection with
sudo -E -u tedge p11tool --provider p11-kit-client.so --list-tokens
```

### using it in %%te%%

Once we're sure our cryptographic token is accessible, we can use it in %%te%% to connect to
Cumulocity. In following steps we assume that `c8y.url` is already set.

13. Build %%te%% with cryptoki support

The HSM support in %%te%% is still experimental, so it's gated behind `cryptoki` build feature
switch. You'll have to build %%te%% with that feature and install the built version on your system.

```sh
cargo build --features cryptoki
```

14. Set required config options

```sh
# Enable authentication using cryptoki
tedge config set device.cryptoki.enable true
# Set the path to the module that will be loaded for authentication
tedge config set device.cryptoki.module_path /usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so
# Set the PIN value (123456 is the default)
tedge config set device.cryptoki.pin 123456
# Use the built-in bridge (using cryptoki is unsupported with mosquitto)
tedge config set mqtt.bridge.built_in true
```

TODO: this doesn't fully work because P11_KIT_SERVER_ADDRESS env var needs to be set for the bridge.
We don't currently have a config setting for that, so one would have to add this to systemd service
definition, but as P11 kit usage is subject to change, I'll leave it for later.

15. Reconnect

```sh
tedge reconnect c8y
```

## References

---

\* Yubikey doesn't fully support all of PKCS#11, but the relevant subset works
