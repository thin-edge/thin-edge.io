# How to register?

## Create self-signed certificate

To create new certificate you can use [`tedge cert create`](../references/tedge-cert.md) thin-edge.io command:

```shell
sudo tedge cert create --device-id alpha
```

> Note: `tedge cert` requires `sudo` privilege. This command provides no output on success.

[`sudo tedge cert create`](../references/tedge-cert.md) will create certificate in a default location (`/etc/tedge/device-certs/`).
To use a custom location, refer to [`tedge config`](../references/tedge-config.md).

Now you should have a certificate in the `/etc/tedge/device-certs/` directory.

```shell
$ ls /etc/tedge/device-certs/
/etc/tedge/device-certs/tedge-certificate.pem
```

### Errors

#### Certificate creation fails due to invalid device id

If non-supported characters are used for the device id then the cert create will fail with below error:

```plain
Error: failed to create a test certificate for the device +.

Caused by:
    0: DeviceID Error
    1: The string '"+"' contains characters which cannot be used in a name [use only A-Z, a-z, 0-9, ' = ( ) , - . : ? % * _ ! @]
```


#### Certificate already exists in the given location

If the certificate already exists you may see following error:

```plain
Error: failed to create a test certificate for the device alpha.

Caused by:
    A certificate already exists and would be overwritten.
            Existing file: "/etc/tedge/device-certs/tedge-certificate.pem"
            Run `tedge cert remove` first to generate a new certificate.
```

> Warning! Removing a certificate can break the bridge and more seriously delete a certificate that was a CA-signed certificate.

Follow the instruction to remove the existing certificate and issue [`tedge cert remove`](../references/tedge-cert.md):

```shell
sudo tedge cert remove
```

and try [`tedge cert create`](../references/tedge-cert.md) once again.

## Next steps

1. [How to connect?](./004_connect.md)
2. [How to use mqtt pub/sub?](./005_pub_sub.md)
