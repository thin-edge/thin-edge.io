# How to register?

## Create self-signed certificate

To create new certificate you can use [`tedge cert create`](../references/tedge-cert.md) thin-edge command:

```shell
tedge cert create --device-id alpha
```

> Note: This command provides no output on success.

[`tedge cert create`](../references/tedge-cert.md) will create certificate in a default location (`/home/user/.tedge/`), to use custom location refer to [`tedge config`](../references/tedge-config.md).

Now you should have a certificate in the `/home/user/.tedge/` directory.

```shell
$ ls ~/.tedge/
/home/user/.tedge/tedge-certificate.pem
```

### Errors

#### Certificate already exists in the given location

If the certificate already exists you may see following error:

```plain
Error: failed to create a test certificate for the device alpha.

Caused by:
    A certificate already exists and would be overwritten.
            Existing file: "/home/user/.tedge/tedge-certificate.pem"
            Run `tegde cert remove` first to generate a new certificate.
```

Follow the instruction to remove the existing certificate and issue [`tedge cert remove`](../references/tedge-cert.md):

```shell
tegde cert remove
```

and try [`tedge cert create`](../references/tedge-cert.md) once again.

## Next steps

1. [How to connect?](./004_connect)
2. [How to use mqtt pub/sub?](./005_pub_sub.md)
