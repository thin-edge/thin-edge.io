# Installation

To install thin edge package it is required to use `curl` to download the package and `dpkg` to install.

### Package download

Thin Edge package is in thin-edge repository on GitHub at: [thin-edge.io](https://github.com/makr11st/thin-edge/releases/).

To download the package form github repository use following command (use desired version):
```shell
$ curl -LJO https://github.com/thin-edge/thin-edge.io/archive/tedge_<version>_<arch>.deb
```
where:
> version -> thin-edge version in x.x.x format

> arch -> required architecture [amd64, armhf]


### Dependency installation

```shell
$ apt install mosquitto
```

### Package installation
```shell
$ dpkg tedge
```

```shell
$ dpkg c8y_mapper
```

Next steps: [Registration](./003_registration.md)
