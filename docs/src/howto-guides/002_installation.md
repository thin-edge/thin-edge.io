# How to install `thin-edge.io`?

## Installation with get-thin-edge_io.sh script

There are two possibilities to install thin-edge.io, the easiest way is to use the installation script with this command:
```
curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s 0.2.0
```
You can execute that command on your device and it will do all required steps for an initial setup.

If you prefer to have a little more control over the installation or the script did not work for you, please go on with the following steps.

## Manual installation steps

To install thin edge package it is required to use `curl` to download the package and `dpkg` to install it.

## Dependency installation

thin-edge.io has single dependency and it is `mosquitto` used for communication southbound and northbound e.g. southbound, devices can publish measurements; northbound, gateway may relay messages to cloud.
`mosquitto` can be installed with your package manager. For apt the command may look as following:

```shell
apt install mosquitto
```

> Note: Some OSes may require you to use `sudo` to install packages.

```shell
sudo apt install mosquitto
```

## `thin-edge.io` installation

When all dependencies are in place you can proceed with installation of `thin-edge.io cli` and `thin-edge.io mapper` service.

### Package download

thin-edge.io package is in thin-edge.io repository on GitHub: [thin-edge.io](https://github.com/thin-edge/thin-edge.io/releases).

To download the package from github repository use following command (use desired version):

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/<package>_<version>_<arch>.deb
```

where:
> `version` -> thin-edge.io version in x.x.x format
>
> `arch` -> architecture type (amd64, armhf)

Eg:

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.1.0/tedge_0.1.0_armhf.deb
```

and for `mapper`:

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.1.0/tedge_mapper_0.1.0_armhf.deb
```

### Package installation

Now, we have downloaded the package we can proceed to installation. First we will install cli tool `tedge`.

> Note: Some OSes may require you to use `sudo` to install packages and therefore all following commands may need `sudo`.

To install `tedge` use following command:

```shell
dpkg -i tedge_<version>_<arch>.deb
```

Eg:

```shell
dpkg -i tedge_0.1.0_armhf.deb
```

To install mapper for thin-edge.io do:

```shell
dpkg -i tedge_mapper_<version>_<arch>.deb
```

Eg:

```shell
dpkg -i tedge_mapper_0.1.0_armhf.deb
```

### Add your user to `tedge-users` group

During the installation process, a `tedge-users` group is automatically created,
in order to ease the administration of who can use the `sudo tedge` command on the device.
Indeed, the `tedge` command needs to be run using `sudo`.
So, unless all the users are granted sudo privileges, you have to add a user to the `tedge-users` group for that user to be able to use `tedge`.

Run this command to add a user to the group.

```shell
sudo adduser <user> tedge-users
``` 

## Next steps

1. [How to register?](./003_registration.md)
2. [How to connect?](./004_connect.md)
