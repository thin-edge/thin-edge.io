---
title: tedge-p11-server
tags: [Reference, Security, "PKCS #11", CLI]
description: "An optional component for accessing PKCS #11 cryptographic tokens"
---

## Description

**tedge-p11-server** is an optional component used by %%te%% for accessing PKCS #11 cryptographic
tokens. It should be used when it's not possible to load the PKCS #11 dynamic module in `tedge`
directly at runtime, e.g. when running a statically linked %%te%% on musl or when `tedge` doesn't
have the permission to view/access cryptographic tokens, e.g. because it runs in a container.

If you can load dynamic objects at runtime and can access the token directly, `tedge-p11-server` can
be used, but you can also use the `module` cryptoki mode, where the module is used directly in
tedge. [See guide](./hsm-support.md#part-2-thin-edge-setup).


## Connecting to the server from tedge

`tedge-p11-server` exposes a UNIX socket `tedge` will connect to to verify the client certificate
when connecting to the cloud. To make `tedge` use `tedge-p11-server` for client authentication, it's
necessary to set the mode to `socket`:

```shell
tedge config set device.cryptoki.mode socket
tedge config set device.cryptoki.socket_path /path/to/socket.sock # default: /run/tedge-p11-server/tedge-p11-server.sock
```

## Running the server

After installing `tedge-p11-server` package, the `tedge-p11-server.socket` unit will be started,
which creates a socket that will activate `tedge-p11-server.service` once `tedge` attempts to
connect to it. To ensure it will be triggered, you can run the following:

```sh
systemctl status tedge-p11-server.service
```

```sh title="systemctl status tedge-p11-server.service"
○ tedge-p11-server.service - tedge-p11-server
     Loaded: loaded (/etc/systemd/system/tedge-p11-server.service; disabled; preset: enabled)
     Active: inactive (dead)
TriggeredBy: ● tedge-p11-server.socket
```

For more information about socket activation, see [systemd.socket(5)][1].

[1]: https://www.freedesktop.org/software/systemd/man/latest/systemd.socket.html

### Invoking the server from the CLI

Alternatively, you can start the server manually from the CLI:

```shell
tedge-p11-server --module-path /path/to/pkcs11-module.so
```
```sh title="Output"
2025-05-06T11:13:40.171073Z  INFO tedge_p11_server: Using cryptoki configuration cryptoki_config=CryptokiConfigDirect { module_path: "/path/to/pkcs11-module.so", pin: "[REDACTED]", uri: None }
2025-05-06T11:12:21.748700Z  INFO tedge_p11_server: Server listening listener=Some("./tedge-p11-server.sock")
```

`tedge-p11-server` will create the socket and delete it when it exits.

### Configuring tedge

After ensuring `tedge` will be able to connect to the socket mounting it to an accessible path and setting up appropriate permissions (connecting clients need to have read/write permissions) set the following `tedge` options:

```sh
tedge config set device.cryptoki.enable socket
tedge config set device.cryptoki.socket_path /path/to/socket.sock
tedge config set device.cryptoki.pin 123456
tedge config set device.cryptoki.uri "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=83f9cf49039c051a;token=my-token;id=%01;object=my-key;type=private"
```

## Key selection

<!-- NOTE: this behaviour is currently not tested directly -->
In addition to the key selection behaviour described on [main HSM reference page](./hsm-support.md#key-selection),
the `tedge-p11-server`, using the `device.cryptoki.uri` option, can be used to set a filter that
narrows down tokens/key objects `tedge` can access. For example, if `device.cryptoki.uri` contains a
URI that identifies a token, then regardless of value of `device.key_uri`, only objects from this
token will be considered for a key.

## Relevant configuration

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

## Command help

<!-- the command component doesn't generate any output, perhaps because tedge-p11-server binary isn't present? -->
```sh command="tedge-p11-server --help" title="tedge-p11-server --help"
thin-edge.io service for passing PKCS#11 cryptographic tokens

Usage: tedge-p11-server [OPTIONS]

Options:
      --socket-path <SOCKET_PATH>
          A path where the UNIX socket listener will be created
          
          [env: TEDGE_DEVICE_CRYPTOKI_SOCKET_PATH]

      --module-path <MODULE_PATH>
          The path to the PKCS#11 module
          
          [env: TEDGE_DEVICE_CRYPTOKI_MODULE_PATH]

      --pin <PIN>
          The PIN for the PKCS#11 token
          
          [env: TEDGE_DEVICE_CRYPTOKI_PIN]

      --uri <URI>
          A URI of the token/object to use.
          
          See RFC #7512.
          
          [env: TEDGE_DEVICE_CRYPTOKI_URI]

      --log-level <LOG_LEVEL>
          Configures the logging level.
          
          One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.

      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
