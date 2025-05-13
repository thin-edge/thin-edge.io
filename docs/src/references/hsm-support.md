---
title: Hardware Security Module (HSM)
tags: [Reference, Security]
description: Current state of HSM support in %%te%%.
---

%%te%% supports HSM using PKCS #11 (_aka_ cryptoki) cryptographic tokens for MQTT client
authentication between the device and the cloud.

With this feature, %%te%% uses an Hardware Security Module (HSM) to store the private key of the
device, preventing this key to be stolen. Device authentication is then delegated by %%te%% to the
module using the `PKCS#11` protocol when a TLS connection is established.

When running `tedge connect` or `tedge reconnect` command, as part of a TLS handshake with the
remote MQTT broker, a proof of ownership of the device certificate is required.
This is achieved by signing a TLS 1.3 CertificateVerify message by the PKCS #11 cryptographic token.
This happens only once when establishing an MQTT connection over TLS and will only need to be
repeated when a new connection is opened.

Any HSM which has a `PKCS#11` interface are supported, some examples of such modules are:  

* USB based devices like NitroKey HSM 2, Yubikey 5  
* TPM 2.0 (Trusted Platform Module)  
* ARM TrustZone (via OP-TEE)  

For now, HSM is only used for the TLS MQTT connection between the device and C8y cloud.  
Additionally, the built-in bridge has to be used and the user has to device certificate corresponds
to private key stored in the HSM (a step that depends on the actual key).

## Configuration

This feature has the following related configuration options:

```sh command="tedge config list --doc device.cryptoki" title="tedge config list --doc device.cryptoki"
       device.cryptoki.mode  Whether to use a Hardware Security Module for authenticating the MQTT connection with the cloud.  "off" to not use the HSM, "module" to use the provided cryptoki dynamic module, "socket" to access the HSM via tedge-p11-server signing service.
                             Examples: off, module, socket

device.cryptoki.module_path  A path to the PKCS#11 module used for interaction with the HSM.  Needs to be set when `device.cryptoki.mode` is set to `module`.
                             Example: /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so

        device.cryptoki.pin  Pin value for logging into the HSM.
                             Example: 123456

        device.cryptoki.uri  A URI of the token/object to be used by tedge-p11-server.  See RFC #7512.
                             Example: pkcs11:token=my-pkcs11-token;object=my-key

device.cryptoki.socket_path  A path to the tedge-p11-server socket.  Needs to be set when `device.cryptoki.mode` is set to `socket`.
                             Example: /run/tedge-p11-server/tedge-p11-server.sock
```

```sh command="tedge config list --doc key_uri" title="tedge config list --doc key_uri"
    device.key_uri  A PKCS#11 URI of the private key.  See RFC #7512.
                    Example: pkcs11:token=my-pkcs11-token;object=my-key

c8y.device.key_uri  A PKCS#11 URI of the private key.  See RFC #7512.
                    Example: pkcs11:token=my-pkcs11-token;object=my-key

 az.device.key_uri  A PKCS#11 URI of the private key.  See RFC #7512.
                    Example: pkcs11:token=my-pkcs11-token;object=my-key

aws.device.key_uri  A PKCS#11 URI of the private key.  See RFC #7512.
                    Example: pkcs11:model=PKCS%2315%20emulated
```

## Setup guide
<!-- split the guide into a separate page under "Operate Devices" category? -->
<!-- also would be nice to write a test for this guide -->

The following guide shows how to connect to Cumulocity using a PKCS #11 cryptographic token. Instead
of using a dedicated hardware token, we'll create a software token using
[SoftHSM2](https://github.com/softhsm/softHSMv2) and import currently used private key on it.

:::note

While this guide uses SoftHSM2 to demonstrate the feature, be aware that in a real production
setting you'll probably be using a different, hardware token. The process of setting up the token
itself may be different for each token type, as well as require using a different PKCS #11 dynamic
library, but in all cases, the goal is to:

- store the private key on the HSM
- store the corresponding certificate on the file system
- set `device.cert_path` to the certificate path
- set `device.cryptoki.module_path` to the correct PKCS #11 dynamic library
- set `device.cryptoki.pin` and `device.cryptoki.uri` accordingly to the local HSM settings

:::

### Step 1: Setup the cryptographic token

1. Install SoftHSM2 (to create the token and key) and `p11tool` (to view the [PKCS #11 URI][p11uri]
   of the key).

    ```sh
    sudo apt-get install -y softhsm2 gnutls-bin
    ```

    [p11uri]: https://www.rfc-editor.org/rfc/rfc7512

For SoftHSM configuration, see [SoftHSM README](https://github.com/softhsm/softHSMv2?tab=readme-ov-file#configure-1).

2. Add tedge and current user to `softhsm` group. Only users belonging to `softhsm` group can view
   and manage SoftHSM tokens. After adding your own user, remember to logout and login for changes
   to take effect. Alternatively, you can just run `softhsm2-util` and `p11tool` with `sudo`.

   ```sh
    sudo usermod -a -G softhsm tedge
    sudo usermod -a -G softhsm $(id -un)
   ```

3. Create a new SoftHSM token. You'll be prompted for a PIN for a regular user and security officer
   (SO). The rest of the guide assumes PIN=123456, but you're free to use a different one.

    ```sh
    softhsm2-util --init-token --slot 0 --label my-token
    ```

4. Import the private key to the created token. Make sure to use the correct PIN value for a regular
   user from the previous step.

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

5. Get the URI of the key

    First, see what tokens are available

    ```sh
    p11tool --list-tokens
    ```

    ```sh title="Output"
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
    ```sh title="Output"
    Object 0:
        URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token;id=%01;object=my-key;type=private
        Type: Private key (EC/ECDSA-SECP256R1)
        Label: my-key
        Flags: CKA_PRIVATE; CKA_SENSITIVE; 
        ID: 01
    ```

### Step 2: thin-edge setup

Next, we're going to configure `tedge` to use the token directly using module mode.

If that mode doesn't work for you, because of the token can't be accessed for some reason or you
can't dynamically load the PKCS #11 library, see how to [use `tedge-p11-server`](./tedge-p11-server.md).

Using the module mode, the cryptoki module will be loaded by the dynamic loader
and used for signing. If there are many tokens or private keys we also need to
provide the URI for the key to select a correct one.

1. Enable the module mode and set the module and the key URI.
    ```sh
    tedge config set device.cryptoki.mode module
    tedge config set device.cryptoki.module_path /usr/lib/x86_64-linux-gnu/softhsm/libsofthsm2.so
    tedge config set device.key_uri "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token;id=%01;object=my-key;type=private"
    ```

    :::note

    `[cloud].device.key_uri` config setting corresponds to the usual `[cloud].device.key_path` setting,
    but instead of pointing to the private key file, it contains the URI for a given cloud.

    :::

### Step 3: Reconnect

1. Enable the built-in bridge. PKCS #11 doesn't work when using mosquitto as a bridge.
    ```sh
    tedge config set mqtt.bridge.built_in true
    ```

2. Reconnect to c8y

    ```sh
    tedge reconnect c8y
    ```

    ```sh title="tedge reconnect c8y"
    Disconnecting from Cumulocity
    Removing bridge config file... ✓
    Disabling tedge-mapper-c8y... ✓
    reconnect to Cumulocity cloud.:
            device id: marcel-hsm-device-rsa
            cloud profile: <none>
            cloud host: thin-edge-io.eu-latest.cumulocity.com:8883
            auth type: Certificate
            certificate file: /etc/tedge/device-certs/rsa/tedge-certificate.pem
            cryptoki: true
            bridge: built-in
            service manager: systemd
            mosquitto version: 2.0.20
            proxy: Not configured
    Creating device in Cumulocity cloud... ✓
    Restarting mosquitto... ✓
    Waiting for mosquitto to be listening for connections... ✓
    Enabling tedge-mapper-c8y... ✓
    Verifying device is connected to cloud... ✓
    Checking Cumulocity is connected to intended tenant... ✓
    Enabling tedge-agent... ✓
    ```

    `cryptoki: true` in the connection summary confirms that we connected using our PKCS #11 token.

## Key selection

<!-- at the moment this isn't tested very extensively -->

`tedge` or `tedge-p11-server` will try to find a private key even if the URI is not provided. In
cases where there are multiple tokens/keys to choose from, the first one returned by the system will
be automatically selected, but appropriate warning will be emitted:

```
WARN tedge_p11_server::pkcs11: Multiple keys were found. If the wrong one was chosen, please use a URI that uniquely identifies a key.
```

In such cases, config setting `device.key_uri` can be used to select an appropriate key or token on
which the key is located.

It is also possible to use a URI that identifies a token in settings like `device.key_uri`. The URI
will then be used to select a token, but the key will be selected automatically, though the selected
key may be wrong if there are multiple to choose from. Also if the URI contain attributes that
identify a key, but doesn't contain attributes that identify a token, still the first token will be
selected, even if another token contains the intended key.
