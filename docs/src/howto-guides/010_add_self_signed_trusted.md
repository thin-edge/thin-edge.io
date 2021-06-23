# How to add self-signed certificate to trusted

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
sudo update-ca-certificates
```

> If you do not have the ca-certificates package, install it with your package manager. For Ubuntu or Raspberry Pi OS that would be to invoke folllowing command:
>
> ```shell
> sudo apt install ca-certificates
> ```
