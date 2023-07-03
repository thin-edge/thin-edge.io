---
title: Self-signed Device Certificate
tags: [Operate, Security]
sidebar_position: 1
---

# How to add self-signed certificate to trusted certificates list

## Overview

If the server you are trying to connect `thin-edge.io` to is presenting a certificate with a root that is not currently trusted, then you can add the server's root certificate to the list of trusted root certificates.
For the most part the store will be filled with certificates from your TLS/SSL provider, but when you want to use self-signed certificates you may need to update your local certificate store.

Below are instructions on how to add new CA certificate and update the certificate store.

:::note
Provided instructions are for supported OSes and may not apply to the flavour you are running,
if you need help with other OS please consult appropriate documentation.
:::

## Debian/Ubuntu/RaspberryPi OS

If you do not have the `ca-certificates` package installed on your system, install it with your package manager.

```sh
sudo apt install ca-certificates
```

To add a self-signed certificate to the trusted certificate repository on thin-edge.io system:

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

Additionally you can check correctness of the installed certificate:

```sh
cat /etc/ssl/certs/ca-certificates.crt | grep -f <full_path_to_the_certificate>
```
