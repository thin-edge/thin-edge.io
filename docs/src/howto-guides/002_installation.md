# Installation

To install thin edge package it is required to use `curl` to download the package and `dpkg` to install it.

## Dependency installation

thin-edge has single dependency and it is `mosquitto` MQTT server, the server is used for communication southband and northbound e.g. southband, devices can publish measurementes; northbound, gateway may relay messages to cloud.
`mosquitto` can be installed with your package manager for apt the command may look as following:

```shell
apt install mosquitto
```

NB: Some OSes may require you to use `sudo` to install packages:

```shell
sudo apt install mosquitto
```

## Thin Edge installation

When all dependencies are in place you can proceed with installation of thin-edge cli and thin-edge mapper service.

### Package download

Thin Edge package is in thin-edge repository on GitHub: [thin-edge.io](https://github.com/thin-edge/thin-edge.io/releases/).

To download the package from github repository use following command (use desired version):

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/archive/tedge_<version>_<arch>.deb
```

where:
> `version` -> thin-edge version in x.x.x format

> `arch` -> required architecture [amd64, armhf]

Eg:

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/archive/tedge_0.1.0_amd64.deb
```

and for `mapper`:

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/archive/c8y_mapper_0.1.0_amd64.deb
```

### Package installation

```shell
dpkg -i tedge_<version>_<arch>
```

Eg:

```shell
dpkg -i tedge_0.1.0_amd64
```

```shell
dpkg -i c8y_mapper_<version>_<arch>
```

Eg:

```shell
dpkg -i c8y_mapper_0.1.0_amd64
```

Next steps:

1. [Registration](./003_registration.md)
2. [Connect](004_connect.md)
3. [Mapping](...)
