---
title: Cloud Authentication
tags: [Operate, Security]
description: Configuring certificates for your cloud connection
---

When %%te%% connects a cloud, the cloud endpoint is authenticated using X.509 certificates.
For that to work, the signing certificate of the cloud certificate must be trusted by the device.
Usually, these certificates are stored in `/etc/ssl/certs` and nothing specific has to done on the device.

A specific configuration will be required only for cloud endpoints which CA is not trusted by the device OS default setting.

## Configuration

Several `tedge config` settings are used by %%te%% to locate the signing certificate of the cloud endpoint. 

- `c8y.root_cert_path`  The path where Cumulocity IoT root certificate(s) are stored (MQTT)
- `c8y.proxy.ca_path`  The path where Cumulocity IoT root certificate(s) are stored (HTTP)
- `aws.root_cert_path`  The path where AWS IoT root certificate(s) are stored (MQTT).
- `az.root_cert_path`  The path where Azure IoT root certificate(s) are stored (MQTT).

All these paths can point to a directory where the trusted certificates are stored
as well as directly to file containing the certificate of the authority signing the cloud endpoint certificate.
Per default, all these paths are set to the system default:  `/etc/ssl/certs`.

## Adding a Root Certificate

If the server you are trying to connect %%te%% to is presenting a certificate with a root that is not currently trusted,
then you can add the server's root certificate to the list of trusted root certificates.
For the most part the store will be filled with certificates from your TLS/SSL provider,
but if this is not the case you may need to update your local certificate store.

:::note
Updating the local certificate store is notably required to connect Cumulocity IoT Edge,
as this distribution of Cumulocity uses self-signed certificates to authenticate itself.
:::

Below are instructions on how to add new CA certificate and update the certificate store.

:::note
Provided instructions are for supported OSes and may not apply to the flavour you are running,
if you need help with other OS please consult appropriate documentation.
:::

### Debian/Ubuntu/RaspberryPi OS

If you do not have the `ca-certificates` package installed on your system, install it with your package manager.

```sh
sudo apt install ca-certificates
```

To add a self-signed certificate to the trusted certificate repository on %%te%% system:

Create a `/usr/local/share/ca-certificates/` directory if it does not exist on your computer:

```sh
sudo mkdir /usr/local/share/ca-certificates/
```

The directory should be owned by `root:root` and have `755` permissions set for it. The certificates files should be `644`.

Copy your root certificate (in `PEM` format with `.crt` extension) to the created directory:

```sh
sudo cp <full_path_to_the_certificate> /usr/local/share/ca-certificates/
```

Install the certificates:

```sh
sudo update-ca-certificates
```

```text title="Output"
Updating certificates in /etc/ssl/certs...
1 added, 0 removed; done.
Running hooks in /etc/ca-certificates/update.d...
done.
```

Check the certificate was correctly installed:

```sh
ls /etc/ssl/certs | grep <certificate_name>
```

Additionally, you can check correctness of the installed certificate:

```sh
cat /etc/ssl/certs/ca-certificates.crt | grep -f <full_path_to_the_certificate>
```
