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

The following guide shows how to connect to Cumulocity using a PKCS #11 cryptographic token. Instead
of using a dedicated hardware token, we'll create a software token using
[SoftHSM2](https://github.com/softhsm/softHSMv2) and import currently used private key on it.

### Part 1: Setup the cryptographic token

1. Install SoftHSM2 and [configure][2] it if necessary.

    ```sh
    apt-get install -y softhsm2
    ```

[2]: https://github.com/softhsm/softHSMv2?tab=readme-ov-file#configure-1

2. Create a new token.
    ```sh
    softhsm2-util --init-token --slot 0 --label my-token
    ```

3. Import the private key to the token. Make sure to use the correct PIN value for a regular user
   from the previous step.

    ```sh
    PUB_PRIV_KEY=$(
        cat "$(tedge config get device.key_path)" && cat "$(tedge config get device.cert_path)"
    )
    softhsm2-util \
        --import <(echo "$PUB_PRIV_KEY") \
        --token my-token \
        --label my-key \
        --id 01 \
        --pin 123456 \
    ```

### Part 2: thin-edge setup

Next, we need to make sure the token is accessible by %%te%%. This can be tricky because of different permissions or if
%%te%% is inside of a container.

So we're going to check if we can use the module directly and do it if so. If not, we're going to
use [`tedge-p11-server`](./tedge-p11-server.md).

5. Install `p11tool`

    ```sh
    apt-get install -y gnutls-bin
    ```

6. Check if SoftHSM2 key object is visible using the module

    <!-- if one needs root to interact with softhsm tokens, running tedge-p11-server as root would
    make them available to tedge, but runing the server as root seems a bad idea and we probably
    shouldn't recommend it-->
    > NOTE: It may be necessary to have root permissions to view SoftHSM tokens. If so, `tedge` user also won't be able
    > to access them.

    First, check if the token itself is visible:

    ```sh
    p11tool --list-tokens
    ```

    ```sh title="p11tool --list-tokens"
    ...
    Token 2:
        URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token
        Label: my-token
        Type: Generic token
        Flags: RNG, Requires login
        Manufacturer: SoftHSM project
        Model: SoftHSM v2
        Serial: 83f9cf49039c051a
        Module: /usr/lib/x86_64-linux-gnu/softhsm/libsofthsm2.so
    ...
    ```

    Now check if the private key object is in the token. You may need to login, provide the regular
    user PIN and also provide token URL(URI) if multiple tokens are connected:

    ```sh
    p11tool --login --set-pin=123456 --list-privkeys "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token"
    ```
    ```sh title="p11tool --login --set-pin=123456 --list-privkeys "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token""
    Object 0:
        URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token;id=%01;object=my-key;type=private
        Type: Private key (EC/ECDSA-SECP256R1)
        Label: my-key
        Flags: CKA_PRIVATE; CKA_SENSITIVE; 
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
tedge config set device.cryptoki.module_path /usr/lib/x86_64-linux-gnu/softhsm/libsofthsm2.so
# optional if there are many tokens/keys
tedge config set device.key_uri "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token;id=%01;object=my-key;type=private"
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
tedge config set device.cryptoki.uri "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token;id=%01;object=my-key;type=private"
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
