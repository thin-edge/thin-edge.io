---
title: Certificate signing request
tags: [Operate, Security, Cloud]
description: Generate certificate signing request for %%te%%
---

If you want to use a device certificate which is signed by a Certificate Authority (CA), you can generate the Certificate Signing Request (CSR), which is later used by CA to generate a device certificate. This process requires additional tooling as %%te%% provides you only with the CSR. 

## Create a certificate signing request

To create a CSR you can use [`tedge cert create-csr`](../../references/cli/tedge-cert.md) %%te%% command:

```sh
sudo tedge cert create-csr --device-id alpha
```

or

```sh
sudo tedge cert create-csr
```
if a device certificate already exists and you want to reuse device name.


```text title="Output"
Certificate Signing Request was successfully created.
```

:::note
`tedge cert` requires `sudo` privilege as creating a private key owned by the MQTT broker. This command provides no output on success.
:::

Now you should have a CSR in the `/etc/tedge/device-certs/` directory:

```sh
ls -l /etc/tedge/device-certs/
```

```text title="Output"
total 8
-r--r--r-- 1 mosquitto mosquitto 664 May 31 09:26 tedge.csr
-r-------- 1 mosquitto mosquitto 246 May 31 09:26 tedge-private-key.pem
```

[`sudo tedge cert create-csr`](../../references/cli/tedge-cert.md) creates the certificate signing request in a default location (`/etc/tedge/device-certs/`). To use a custom location, refer to [`tedge config`](../../references/cli/tedge-config.md) or provide absolute path as a command argument:

```sh
sudo tedge cert create-csr --device-id alpha --output-path /custom/path/mycsr.csr
```

:::note
`tedge cert create-csr` will reuse the private key if already created, e.g by the `tedge cert create` command.
:::

To check the content of CSR, you can use external tools, like `openssl`.

```sh
openssl req -in /etc/tedge/device-certs/tedge.csr -noout -text
```

```text title="Output"
Certificate Request:
    Data:
        Version: 1 (0x0)
        Subject: CN = alpha, O = Thin Edge, OU = Test Device
        Subject Public Key Info:
            Public Key Algorithm: id-ecPublicKey
                Public-Key: (256 bit)
                pub:
                    04:95:e6:48:48:9b:8e:03:a0:fd:07:41:e6:e7:25:
                    21:3b:ed:c7:8d:13:2f:69:a7:94:17:43:7c:da:ca:
                    33:fb:bb:93:fe:eb:c1:50:65:c2:47:70:87:5e:ab:
                    a3:d5:ec:9b:5c:65:7a:ba:7d:92:20:a1:80:9b:d6:
                    79:71:be:15:56
                ASN1 OID: prime256v1
                NIST CURVE: P-256
        Attributes:
            (none)
            Requested Extensions:
    Signature Algorithm: ecdsa-with-SHA256
    Signature Value:
        30:46:02:21:00:81:28:11:28:9b:92:cb:b8:d9:d2:1c:3c:8d:
        00:1f:4e:44:ae:ba:61:7f:ca:17:75:d9:d4:11:04:fa:11:e8:
        a2:02:21:00:f2:f4:11:77:5c:32:c8:d5:86:66:29:d5:ae:27:
        3f:64:31:be:f8:4a:89:29:bf:e0:01:b4:f2:63:1f:f0:f0:fb
```

## Errors

### Certificate Signing Request creation fails due to invalid device id

If non-supported characters are used for the device id then the cert create-csr will fail with below error:

```text
Error: failed to Generate the Certificate Signing Request.

Caused by:
    0: DeviceID Error
    1: The string '"+"' contains characters which cannot be used in a name [use only A-Z, a-z, 0-9, ' = ( ) , - . ? % * _ ! @]
```
