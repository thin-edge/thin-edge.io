# How to install `thin-edge.io`?

To install thin edge package it is required to use `curl` to download the package and `dpkg` to install it.

## Dependency installation

thin-edge.io has single dependency and it is `mosquitto` used for communication southbound and northbound e.g. southbound, devices can publish measurements; northbound, gateway may relay messages to cloud.
`mosquitto` can be installed with your package manager. For apt the command may look as following:

```shell
apt install mosquitto
```

> Note: Some OSes may require you to use `sudo` to install packages:

```shell
sudo apt install mosquitto
```

## `thin-edge.io` installation

When all dependencies are in place you can proceed with installation of `thin-edge.io cli` and `thin-edge.io mapper` service.

### Package download

thin-edge.io package is in thin-edge.io repository on GitHub: [thin-edge.io](https://github.com/thin-edge/thin-edge.io/releases/).

To download the package from github repository use following command (use desired version):

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/archive/<package>_<version>_<arch>.deb
```

where:
> `version` -> thin-edge.io version in x.x.x format
>
> `arch` -> architecture type [amd64, armhf]

Eg:

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/archive/tedge_0.1.0_amd64.deb
```

and for `mapper`:

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/archive/mapper_0.1.0_amd64.deb
```

### Package installation

Now, we have downloaded the package we can proceed to installation. First we will install cli tool `tedge`.

> Note: Some OSes may require you to use `sudo` to install packages and therefore all following commands may need `sudo`.

To install `tedge` use following command:

```shell
dpkg -i tedge_<version>_<arch>
```

Eg:

```shell
dpkg -i tedge_0.1.0_amd64
```

To install mapper for thin-edge.io do:

```shell
dpkg -i mapper_<version>_<arch>
```

Eg:

```shell
dpkg -i mapper_0.1.0_amd64
```

## Next steps

1. [How to register?](./003_registration.md)
2. [How to connect?](./004_connect.md)
3. [How to use mapper?](...)
