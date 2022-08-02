# How to install `thin-edge.io`?

## Installation with get-thin-edge_io.sh script

There are two possibilities to install thin-edge.io, the easiest way is to use the installation script with this command:

```shell
curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
```

You can execute that command on your device and it will do all required steps for an initial setup.

> Note: If you want to get a specific version, add `<version>` at the end like below. 
> `<version>` is consist of 3 digits, e.g. `0.7.3`.
> ```shell
> curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s <version>
> ```

If you prefer to have a little more control over the installation or the script did not work for you,
please go on with the [manual installation steps](#manual-installation-steps).

## Upgrade thin-edge.io with get-thin-edge_io.sh script

If you already have `thin-edge.io` on your device, to upgrade `thin-edge.io`,
the easiest way is to use the same script as the installation. Follow the steps below.
there is no need to remove old version.

> Note: To successfully upgrade `thin-edge.io` all thin-edge.io components **must be stopped**. The components are:
> `tedge-mapper-c8y`, `tedge-mapper-az`, `tedge-mapper-collectd`, `tedge-agent`, `c8y-log-plugin`, `c8y-configuration-plugin`.
>
> To stop `tedge-mapper-c8y`, `tedge-agent`, `tedge-mapper-az`, you can simply run the commands below.
> 
> ```shell
> sudo tedge disconnect c8y
> sudo tedge disconnect az
> ```
> 
> To stop each component one by one, this is an example how to stop them with `systemctl`:
> 
> ```shell
> systemctl stop tedge-mapper-c8y
> systemctl stop tedge-agent
> systemctl stop c8y-log-plugin
> ```

Then, run `get-thin-edge_io.sh` script as below to upgrade to the latest version.

```shell
curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
```

> Note: If you want to upgrade to a specific version, add `<version>` at the end like below.
> `<version>` is consist of 3 digits, e.g. `0.7.3`.
> ```shell
> curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s <version>
> ```

## thin-edge.io manual installation

To install thin edge package it is required to use `curl` to download the package and `dpkg` to install it.

### Dependency installation

thin-edge.io has single dependency and it is `mosquitto` used for communication southbound and northbound e.g. southbound, devices can publish measurements; northbound, gateway may relay messages to cloud.
`mosquitto` can be installed with your package manager. For apt the command may look as following:

```shell
apt install mosquitto
```

> Note: Some OSes may require you to use `sudo` to install packages.

```shell
sudo apt install mosquitto
```

### thin-edge.io package download

thin-edge.io package is in thin-edge.io repository on GitHub: [thin-edge.io](https://github.com/thin-edge/thin-edge.io/releases).

To download the package from github repository use following command (use desired version):

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/<package>_<version>_<arch>.deb
```

where:
> `version` -> thin-edge.io version in x.x.x format
>
> `arch` -> architecture type (amd64, armhf, arm64)

Eg:

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.5.0/tedge_0.5.0_armhf.deb
```

and for `mapper`:

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.5.0/tedge_mapper_0.5.0_armhf.deb
```

### thin-edge.io package installation

Now, we have downloaded the package we can proceed to installation. First we will install cli tool `tedge`.

> Note: Some OSes may require you to use `sudo` to install packages and therefore all following commands may need `sudo`.

To install `tedge` use following command:

```shell
dpkg -i tedge_<version>_<arch>.deb
```

Eg:

```shell
dpkg -i tedge_0.5.0_armhf.deb
```

To install mapper for thin-edge.io do:

```shell
dpkg -i tedge_mapper_<version>_<arch>.deb
```

Eg:

```shell
dpkg -i tedge_mapper_0.5.0_armhf.deb
```

## Next steps

1. [Connect your device to Cumulocity IoT](../tutorials/connect-c8y.md)
2. [Connect your device to Azure IoT](../tutorials/connect-azure.md)
