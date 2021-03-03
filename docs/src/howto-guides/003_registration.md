# Registration

## Create self-signed certificate

NB: DO NOT USE IN PRODUCTION!​

To create new certificate you can use [`tedge cert create`](../references/tedge-cert.md) thin-edge command:

```shell
tedge cert create --device-id alpha
```

> NB: This command provides no output on success.

Now you should have a certificate in the `.tedge` directory.

```shell
$ ls ~/.tedge
/home/user/.tedge/tedge-certificate.pem
```

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

1. [Connect](./004_connect)
