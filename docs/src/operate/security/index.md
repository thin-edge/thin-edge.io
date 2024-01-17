---
title: Security and Access Control
tags: [Operate, Security]
sidebar_position: 1
---

import DocCardList from '@theme/DocCardList';

Thin-edge uses X.509 certificates as the key mechanism to authenticate peers.
- The MQTT connection between the gateway device and the cloud is established over TLS
  and uses certificates to authenticate the device on the cloud, as well as to authenticate the cloud on the device.
- The local MQTT connections, from the miscellaneous services and child devices to the local MQTT broker,
  can also be configured to be established over TLS. In the stronger setting, the clients have to authenticate themselves using certificates.
- The local HTTP services (namely the [File Transfer Service](../../references/tedge-file-transfer-service.md) and the [Cumulocity Proxy](../../references/tedge-cumulocity-proxy.md))
  can be configured to use HTTPS. As for MQTT, certificate-based authentication of the clients can also be enforced.

A complete setting requires numerous private keys, certificates and trust chains.
Nothing really complex, but this requires rigorous settings.
It is therefore recommended to set things up step by step.
- The only mandatory step is to configure the authentication between the gateway device and the cloud.
  - This can be done using a [self-signed device certificate](self_signed_device_certificate.md) or a proper [CA-signed certificate](device-certificate.md).
  - Most of the time the cloud certificate will be trusted out-of-the-box,
    but a [self-signed cloud certificate](cloud_authentication.md) will need specific care.
- The second step is to enable TLS on the local MQTT and HTTP connections.
- The final step is to enforce certificate-based client authentication on the local MQTT and HTTP connections.

<DocCardList />
