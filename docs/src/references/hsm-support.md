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

The options below are used to configure the _P11 provider_, a component which directly interacts
with PKCS 11 devices and makes them available to tedge and other subcomponents, as well as to select
the provider that tedge will use.

There are 2 providers:

- `module`: tedge will load the a P11 library directly. Can be used when PKCS 11 cryptographic
  tokens are directly reachable
- `socket`: tedge will connect to [`tedge-p11-server`](../references/tedge-p11-server.md) proxy via
  a UNIX socket. This can be used when cryptographic tokens are not accessible from `tedge`, for
  example when running inside an isolated container.

Settings like `cryptoki.uri` and `cryptoki.pin` are the default values that the provider will use if
a consumer does not provide them. In practice, this is only relevant to `tedge-p11-server` provider,
which runs in a separate process and can handle many clients. If desired, it can be configured to
use a default pin or limit the scope of tokens available to clients.

```text command="tedge config list --doc device.cryptoki" title="tedge config list --doc device.cryptoki"
       device.cryptoki.mode  Whether to use a Hardware Security Module for authenticating the MQTT connection with the cloud.  "off" to not use the HSM, "module" to use the provided cryptoki dynamic module, "socket" to access the HSM via tedge-p11-server signing service. 
                             Examples: off, module, socket
device.cryptoki.module_path  A path to the PKCS#11 module used for interaction with the HSM.  Needs to be set when `device.cryptoki.mode` is set to `module`. 
                             Example: /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so
        device.cryptoki.pin  A default User PIN value for logging into the PKCS11 token.  May be overridden on a per-key basis using device.key_pin config setting. 
                             Example: 123456
        device.cryptoki.uri  A URI of the token/object to be used by tedge-p11-server.  If set, tedge-p11-server will by default use this URI to select a key for signing if a client does not provide its URI in the request. If the client provides the URI, then the attributes of this server URI will be used as a base onto which client-provided URI attributes will be appended, potentially limiting the scope of keys or tokens that can be used by the clients.  For example, if `cryptoki.uri=pkcs11:token=token1` and `device.key_uri=pkcs11:token2;object=key1`, `tedge-p11-server` will use URI `pkcs11:token1;object=key1`.  For more information about PKCS11 URIs, see RFC7512. 
                             Example: pkcs11:token=my-pkcs11-token;object=my-key
device.cryptoki.socket_path  A path to the tedge-p11-server socket.  Needs to be set when `device.cryptoki.mode` is set to `socket`. 
                             Example: /run/tedge-p11-server/tedge-p11-server.sock
```

The options below are used by the consumer (tedge) to ask `tedge-p11-server` for a specific key. You
can use different keys for different connection profiles. `tedge-p11-server` may limit what keys and
tokens are available.

```text command="tedge config list --doc key_uri" title="tedge config list --doc key_uri"
    device.key_uri  A PKCS#11 URI of the private key.  See RFC #7512.
                    Example: pkcs11:token=my-pkcs11-token;object=my-key

c8y.device.key_uri  A PKCS#11 URI of the private key.  See RFC #7512.
                    Example: pkcs11:token=my-pkcs11-token;object=my-key

 az.device.key_uri  A PKCS#11 URI of the private key.  See RFC #7512.
                    Example: pkcs11:token=my-pkcs11-token;object=my-key

aws.device.key_uri  A PKCS#11 URI of the private key.  See RFC #7512.
                    Example: pkcs11:model=PKCS%2315%20emulated
```

The options below are used by the consumer (tedge) to use a given PIN with a given key, instead of
using a default PIN that `tedge-p11-server` is configured to use.

```text command="tedge config list --doc key_pin" title="tedge config list --doc key_pin"
    device.key_pin  User PIN value for logging into the PKCS#11 token provided by the consumer.  This differs from cryptoki.pin in that cryptoki.pin is used by PKCS#11 provider, e.g. tedge-p11-server as a default PIN for all tokens, but device.key_pin is the PIN provided by the consumer (tedge) with a given `key_uri`.  In practice, this can be used to define separate keys and separate PINs for different connection profiles.
                    Examples: 123456, my-pin
c8y.device.key_pin  User PIN value for logging into the PKCS#11 token provided by the consumer.  This differs from cryptoki.pin in that cryptoki.pin is used by PKCS#11 provider, e.g. tedge-p11-server as a default PIN for all tokens, but device.key_pin is the PIN provided by the consumer (tedge) with a given `key_uri`.  In practice, this can be used to define separate keys and separate PINs for different connection profiles.
                    Examples: 123456, my-pin
 az.device.key_pin  User PIN value for logging into the PKCS#11 token provided by the consumer.  This differs from cryptoki.pin in that cryptoki.pin is used by PKCS#11 provider, e.g. tedge-p11-server as a default PIN for all tokens, but device.key_pin is the PIN provided by the consumer (tedge) with a given `key_uri`.  In practice, this can be used to define separate keys and separate PINs for different connection profiles.
                    Examples: 123456, my-pin
aws.device.key_pin  User PIN value for logging into the PKCS#11 token provided by the consumer.  This differs from cryptoki.pin in that cryptoki.pin is used by PKCS#11 provider, e.g. tedge-p11-server as a default PIN for all tokens, but device.key_pin is the PIN provided by the consumer (tedge) with a given `key_uri`.  In practice, this can be used to define separate keys and separate PINs for different connection profiles.
                    Examples: 123456, my-pin
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

### Step 1: Setup the cryptographic token {#step-1-hsm-setup}

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

### Step 2: %%te%% setup {#step-2-tedge-setup}

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

### Step 3: Reconnect {#step-3-reconnect}

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

## Key selection {#key-selection}

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

## Key generation

```sh command="tedge cert create-key-hsm --help" title="tedge cert create-key-hsm --help"
Generate a new keypair on the PKCS #11 token and select it to be used.

Can be used to generate a keypair on the TOKEN. If TOKEN argument is not provided, the command prints the available tokens.

If TOKEN is provided, the command generates an RSA or an ECDSA keypair on the token. When using RSA, `--bits` is used to set the size of the key, when using ECDSA, `--curve` is used.

After the key is generated, tedge config is updated to use the new key using `device.key_uri` property. Depending on the selected cloud, we use `device.key_uri` setting for that cloud, e.g. `create-key-hsm c8y` will write to `c8y.device.key_uri`.

Usage: tedge cert create-key-hsm [OPTIONS] [TOKEN] [COMMAND]

Commands:
  c8y   
  az    
  aws   
  help  Print this message or the help of the given subcommand(s)

Arguments:
  [TOKEN]
          The URI of the token where the keypair should be created.
          
          If this argument is missing, a list of available initialized tokens will be shown. The token needs to be initialized to be able to generate keys.

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --label <LABEL>
          Human readable description (CKA_LABEL attribute) for the key
          
          [default: tedge]

      --debug
          Turn-on the DEBUG log level.
          
          If off only reports ERROR, WARN, and INFO, if on also reports DEBUG

      --id <ID>
          Key identifier for the keypair (CKA_ID attribute).
          
          If provided and no object exists on the token with the same ID, this will be the ID of the new keypair. If an object with this ID already exists, the operation will return an error. If not provided, a random ID will be generated and used by the keypair.
          
          The id shall be provided as a sequence of hex digits without `0x` prefix, optionally separated by spaces, e.g. `--id 010203` or `--id "01 02 03"`.

      --log-level <LOG_LEVEL>
          Configures the logging level.
          
          One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
          
          Overrides `--debug`

      --type <TYPE>
          The type of the key
          
          [default: ecdsa]
          [possible values: rsa, ecdsa]

      --bits <BITS>
          The size of the RSA keys in bits. Should only be used with --type rsa
          
          [default: 2048]
          [possible values: 2048, 3072, 4096]

      --curve <CURVE>
          The curve (size) of the ECDSA key. Should only be used with --type ecdsa
          
          [default: p256]
          [possible values: p256, p384]

      --pin <PIN>
          User PIN value for logging into the PKCS #11 token.
          
          This flag can be used to provide a PIN when creating a new key without needing to update tedge-config, which can be helpful when initializing keys on new tokens.
          
          Note that in contrast to the URI of the key, which will be written to tedge-config automatically when the keypair is created, PIN will not be written automatically and may be needed to written manually using tedge config set (if not using tedge-p11-server with the correct default PIN).

      --outfile-pubkey <OUTFILE_PUBKEY>
          Path where public key will be saved when a keypair is generated

  -h, --help
          Print help (see a summary with '-h')
```

`tedge cert create-key-hsm` command generates a new keypair on the PKCS #11 token.

1. Configure cryptoki in `module` or `socket` mode as described in previous sections.
2. Run the `tedge cert create-key-hsm` command. You'll need to provide key type, size and label of the
   key object.

    ```sh
    tedge cert create-key-hsm --type ecdsa --curve p256 --label my-key
    ```

    ```sh title="Output"
    New keypair was successfully created.
    Key URI: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=a30ed1ca6244fc5f;token=test-token;id=%51%05%87%75%6F%B7%28%EC%5E%5D%1F%B8%EB%CF%FD%96%B7%E4%28%B6;object=my-key
    Public key:
    -----BEGIN PUBLIC KEY-----
    BEsjmiXDdko90IDdjlAb/bWyTf6kd6S+/KPlj2Yd3zjHZe54evLyHJ1e8dSDhpy7
    2Tcml9ZcHWBHA+MM0NFAbaw=
    -----END PUBLIC KEY-----


    Value of `device.key_uri` was updated to point to the new key
    ```

3. Run `tedge config get device.key_uri` to confirm tedge will use the new key.

    ```sh
    tedge config get device.key_uri
    ```
    pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=a30ed1ca6244fc5f;token=test-token;id=%51%05%87%75%6F%B7%28%EC%5E%5D%1F%B8%EB%CF%FD%96%B7%E4%28%B6;object=my-key
    ```sh title="Output"

Now you're free to use the new key to either request a signed certificate using a CSR or to create a
self-signed certificate.
