---
title: Hardware Security Module (HSM) support
tags: [Reference, Security]
description: Current state of HSM support in %%te%%.
draft: true
---

# Hardware Security Module (HSM) support

TODO:

- [ ] user setup guide

This document describes the state of HSM support of %%te%%.

Currently we support PKCS#11(aka cryptoki) capable modules for MQTT client
authentication between the device and the cloud.

Environment-related prerequisites:

- a PKCS#11-capable device (e.g. Yubikey*)
- a way to make the module accessible to tedge user and tedge processes
- C runtime capable of loading dynamic modules (e.g. glibc or dynamically linked
  musl work, but statically linked musl doesn't work)

On the %%te%% side, we use the private key on the module to sign necessary
messages to establish a TLS1.3 connection. The client certificate is still read
from the file. It is up to the user to ensure the private key in the HSM
corresponds to the used public certificate.

For now, the module is only used for the TLS MQTT connection between the device
and C8y cloud. Additionally, the built-in bridge has to be used.

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

## References

---

\* Yubikey doesn't fully support all of PKCS#11, but the relevant subset works
