---
title: Cloud Authentication
tags: [Operate, Security, Cloud]
sidebar_position: 1
---

# How to register?

## Create self-signed certificate

To create new certificate you can use [`tedge cert create`](../../references/cli/tedge-cert.md) thin-edge.io command:

```sh
sudo tedge cert create --device-id alpha
```

```text title="Output"
Certificate was successfully created
```

:::note
`tedge cert` requires `sudo` privilege. This command provides no output on success.
:::

[`sudo tedge cert create`](../../references/cli/tedge-cert.md) will create certificate in a default location (`/etc/tedge/device-certs/`).
To use a custom location, refer to [`tedge config`](../../references/cli/tedge-config.md).

Now you should have a certificate in the `/etc/tedge/device-certs/` directory.

```sh
ls -l /etc/tedge/device-certs/
```

```text title="Output"
total 8
-r--r--r-- 1 mosquitto mosquitto 664 May 31 09:26 tedge-certificate.pem
-r-------- 1 mosquitto mosquitto 246 May 31 09:26 tedge-private-key.pem
```

## Errors

### Certificate creation fails due to invalid device id

If non-supported characters are used for the device id then the cert create will fail with below error:

```text
Error: failed to create a test certificate for the device +.

Caused by:
    0: DeviceID Error
    1: The string '"+"' contains characters which cannot be used in a name [use only A-Z, a-z, 0-9, ' = ( ) , - . ? % * _ ! @]
```


### Certificate already exists in the given location

If the certificate already exists you may see following error:

```text
Error: failed to create a test certificate for the device alpha.

Caused by:
    A certificate already exists and would be overwritten.
            Existing file: "/etc/tedge/device-certs/tedge-certificate.pem"
            Run `tedge cert remove` first to generate a new certificate.
```

:::note
Removing a certificate can break the bridge and more seriously delete a certificate that was a CA-signed certificate.
:::

Follow the instruction to remove the existing certificate and issue [`tedge cert remove`](../../references/cli/tedge-cert.md):

```sh
sudo tedge cert remove
```

```text title="Output"
Certificate was successfully removed
```

Afterwards, try executing [`tedge cert create`](../../references/cli/tedge-cert.md) again.

## Next steps

1. [How to connect?](../connection/connect.md)
2. [How to use mqtt pub/sub?](../telemetry/pub_sub.md)
