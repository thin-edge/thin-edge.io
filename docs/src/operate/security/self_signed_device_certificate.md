---
title: Self-signed Device Certificate
tags: [Operate, Security, Cloud]
sidebar_position: 1
---

Using a self-signed device certificate is the simplest way to connect a thin-edge device to the cloud.
This is a secure method even if more adapted for testing purposes.
Indeed, the self-signed certificates must be trusted individually by the cloud tenant,
raising managing issues when there are more than a few devices.

## Create self-signed certificate

To create a new certificate you can use [`tedge cert create`](../../references/cli/tedge-cert.md) thin-edge.io command:

```sh
sudo tedge cert create --device-id alpha
```

```text title="Output"
Certificate was successfully created
```

:::note
`tedge cert` requires `sudo` privilege. This command provides no output on success.
:::

[`sudo tedge cert create`](../../references/cli/tedge-cert.md) creates the certificate in a default location (`/etc/tedge/device-certs/`).
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

## Make the cloud trust the device self-signed certificate

For the cloud to trust the device certificate,
the signing certificate must be added to the trusted list of signing certificate of the cloud tenant.

The certificate created with `tedge cert create` being self-signed, one needs to add the device certificate itself to the trusted list.

How this is done depends on the cloud. In the specific case of Cumulocity, this can be done using the `tedge` cli.

One has first to set the Cumulocity end-point:

```sh
tedge config set c8y.url <domain-name-of-your-cumulocity-tenant>
```

And then upload the signing certificate:

```sh
tedge cert upload c8y --user <user-allowed-to-add-trusted-certificate>
```

## Renew self-signed certificate

To renew the expired certificate you can use [`tedge cert renew`](../../references/cli/tedge-cert.md) thin-edge.io command:

```sh
sudo tedge cert renew
```

```text title="Output"
Certificate was successfully renewed, for un-interrupted service, the certificate has to be uploaded to the cloud
```

:::note
`tedge cert renew` will get the device-id from the existing expired certificate and then renews it.
:::

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
