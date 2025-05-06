---
title: 🚧 Hardware Security Module (HSM) support
tags: [Reference, Security]
description: Current state of HSM support in %%te%%.
draft: true
---

%%te%% supports using [PKCS #11][1] (_aka_ cryptoki) cryptographic tokens for MQTT client
authentication between the device and the cloud.

On the %%te%% side, we use the private key on the module to sign necessary messages to establish a
TLS1.3 connection. The client certificate is still read from the file. It is up to the user to
ensure the private key in the HSM corresponds to the used public certificate.

For now, the module is only used for the TLS MQTT connection between the device and C8y cloud.
Additionally, the built-in bridge has to be used.

## Configuration

This feature has the following related configuration options:

```sh command="tedge config list --doc device.cryptoki" title="tedge config list --doc device.cryptoki"
```

```sh command="tedge config list --doc key_uri" title="tedge config list --doc key_uri"
```

## Setup guide
<!-- split the guide into a separate page under "Operate Devices" category? -->
<!-- maybe it would be better to use SoftHSM2 instead of a Yubikey so everyone can follow? -->
<!-- also would be nice to write a test for this guide -->

The following guide shows how to connect to Cumulocity using a PIV-capable Yubikey. The guide uses a
Yubikey 5C NFC.

Environment-related prerequisites:

- a PKCS#11-capable device (e.g. Yubikey*)
- a way to make the module accessible to tedge user and tedge processes
- C runtime capable of loading dynamic modules (e.g. glibc or dynamically linked musl work, but
  statically linked musl doesn't work)

### Part 1: Setup the cryptographic token

1. Make sure the Yubikey is connected.
<!--is it possible to use a generic p11 client to setup the token instead of a yubikey-specific client?-->
2. Install the [ykman CLI](https://docs.yubico.com/software/yubikey/tools/ykman/).
3. Create a private key on the device. In the guide we'll import the private key, but you can also
   create one directly on the Yubikey to ensure its contents are not exposed.

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

### Part 2: thin-edge setup

Next, we need to make sure the Yubikey is accessible as a PKCS #11 token by %%te%%. This can be
tricky because of different permissions or if %%te%% is inside of a container.

So we're going to check if we can use the module directly and do it if so. If not, we're going to
use [`tedge-p11-server`](./tedge-p11-server.md).

5. Install `p11tool` and `opensc-pkcs11`

    ```sh
    apt-get install -y gnutls-bin opensc-pkcs11
    ```

6. Check if Yubikey private key is visible using the module

    First, check if Yubikey token is visible:

    ```sh
    p11tool --list-tokens
    ```

    ```sh title="p11tool --list-tokens"
    ...
    Token 1:
        URL: pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II;serial=f829510c18935023;token=marcel-hsm-device
        Label: marcel-hsm-device
        Type: Hardware token
        Flags: RNG, Requires login
        Manufacturer: piv_II
        Model: PKCS#15 emulated
        Serial: f829510c18935023
        Module: opensc-pkcs11.so
    ...
    ```

    The token should have the same label as the Common Name of the certificate.

    Now check if the private key object is in the token. You may need to login, provide the regular
    user PIN and also provide token URL(URI) if multiple tokens are connected:

    ```sh
    p11tool --login --set-pin=123456 --list-privkeys "pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II;serial=f829510c18935023;token=marcel-hsm-device"
    ```
    ```sh title="p11tool --login --set-pin=123456 --list-privkeys "pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II;serial=f829510c18935023;token=marcel-hsm-device""
    Object 0:
        URL: pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II;serial=f829510c18935023;token=marcel-hsm-device;id=%01;object=PIV%20AUTH%20key;type=private
        Type: Private key (EC/ECDSA)
        Label: PIV AUTH key
        Flags: CKA_PRIVATE; CKA_NEVER_EXTRACTABLE; CKA_SENSITIVE; 
        ID: 01
    ```

    If you can see the private key object, you can use the `module` mode and follow instructions in
    [Part 2a](#part-2a-configure-thin-edge-in-module-mode). Otherwise, use the `socket` mode and
    follow instructions in [Part 2b](#part-2b-use-tedge-p11-server).

### Part 2a: Configure thin-edge in module mode

Using the module mode, the cryptoki module will be loaded by the dynamic loader and used for signing. If there are many
tokens or private keys we also need to provide the URI for the key to select a correct one.

```sh
tedge config set device.cryptoki.mode module
tedge config set device.cryptoki.module_path /usr/lib/x86_64-linux-gnu/pkcs11/opensc-pkcs11.so
# optional if there are many tokens/keys
tedge config set device.key_uri "pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II;serial=f829510c18935023;token=marcel-hsm-device;id=%01;object=PIV%20AUTH%20key;type=private"
```

### Part 2b: Use `tedge-p11-server`

If we can't use the module mode, we can use `tedge-p11-server`.

1. Install `tedge-p11-server`
    ```sh
    apt install -y tedge-p11-server
    ```

2. Check that `tedge-p11-server` service will be activated by the socket
    ```sh title="systemctl status tedge-p11-server"
    ○ tedge-p11-server.service - tedge-p11-server
        Loaded: loaded (/lib/systemd/system/tedge-p11-server.service; disabled; preset: enabled)
        Active: inactive (dead)
    TriggeredBy: ● tedge-p11-server.socket
    ```
    ```sh title="systemctl status tedge-p11-server.socket"
    ● tedge-p11-server.socket - tedge-p11-server socket
        Loaded: loaded (/lib/systemd/system/tedge-p11-server.socket; enabled; preset: enabled)
        Active: active (listening) since Tue 2025-05-06 15:02:03 UTC; 8min ago
    Triggers: ● tedge-p11-server.service
        Listen: /run/tedge-p11-server/tedge-p11-server.sock (Stream)
        Tasks: 0 (limit: 5478)
        Memory: 0B
            CPU: 322us
        CGroup: /system.slice/tedge-p11-server.socket

    May 06 15:02:03 32df1eef46c8 systemd[1]: Starting tedge-p11-server.socket - tedge-p11-server socket...
    May 06 15:02:03 32df1eef46c8 systemd[1]: Listening on tedge-p11-server.socket - tedge-p11-server socket.
    ```

3. Ensure `tedge` will be able to connect to the socket
    - make the socket available to `tedge` by mounting it
    - Set up correct permissions; connecting clients need to have read/write permissions

4. Set required config options

```sh
tedge config set device.cryptoki.enable socket
tedge config set device.cryptoki.socket_path /path/to/socket.sock
tedge config set device.cryptoki.pin 123456
tedge config set device.cryptoki.uri "pkcs11:model=PKCS%2315%20emulated;manufacturer=piv_II;serial=f829510c18935023;token=marcel-hsm-device;id=%01;object=PIV%20AUTH%20key;type=private"
```

### Part 3: Reconnect

1. Enable the builtin bridge. PKCS #11 doesn't work when using mosquitto as a bridge.
```sh
tedge config set mqtt.bridge.built_in true
```

2. Reconnect to c8y

```sh
tedge reconnect c8y
```

## References

---

\* Yubikey doesn't fully support all of PKCS#11, but the relevant subset works

[1]: https://docs.oasis-open.org/pkcs11/pkcs11-base/v3.0/pkcs11-base-v3.0.html
