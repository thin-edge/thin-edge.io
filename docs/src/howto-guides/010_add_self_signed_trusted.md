# How to add self-signed certificate to trusted certicates list

## Overview

For secure connectivity to work you need to add CA certificate to the trusted list repository.
For the most part the store will be filled, but when you want to use self-signed certificates you may need to udpate your local store.
Below are instructions on how to add new CA certificate and update the store.

> Please note: Provided instructions are for supported OSes and may not apply to the flavour you are running, if you need help with other OS please consult appropriate documentation.

## Ubuntu (and Raspberry Pi OS)

> If you do not have the ca-certificates package installed on your system, install it with your package manager. For Ubuntu or Raspberry Pi OS that would be to invoke folllowing command:
>
> ```shell
> sudo apt install ca-certificates
> ```

To manually add a self-signed certificate to be trusted by Linux system:

Create a `/usr/local/share/ca-certificates/` directory if it does not exist on your computer:

```shell
mkdir /usr/local/share/ca-certificates/
```

Copy your root certificate (.crt file) to the created directory:

```shell
cp <full_path_to_the_certificate> /usr/local/share/ca-certificates/
```

Update the certificates:

```shell
$ sudo update-ca-certificates
Updating certificates in /etc/ssl/certs...
1 added, 0 removed; done.
Running hooks in /etc/ca-certificates/update.d...
done.
```
