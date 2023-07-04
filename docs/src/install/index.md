---
title: Install
tags: [Installation]
sidebar_position: 1
---

# How to install thin-edge.io?

## Install via script

There are two possibilities to install thin-edge.io, the easiest way is to use the installation script with this command:

```sh
curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
```

You can execute that command on your device and it will do all required steps for an initial setup.

:::note
If you want to get a specific version, append the `<version>` 3 digits (e.g. `0.7.3`):

```sh
curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s <version>
```
:::

If you prefer to have a little more control over the installation or the script did not work for you,
please go on with the [manual installation steps](#manual-installation-steps).

## Upgrade via script

If you already have `thin-edge.io` on your device, to upgrade `thin-edge.io`,
the easiest way is to use the same script as the installation. Follow the steps below.
There is no need to remove old version.

:::note
For version **0.7.7 or lower, to upgrade `thin-edge.io` from these versions,
all thin-edge.io components **must be stopped** before upgrading.

The components are:
`tedge-mapper-c8y`, `tedge-mapper-az`, `tedge-mapper-collectd`, `tedge-agent`, `tedge-watchdog`, `c8y-log-plugin`, `c8y-configuration-plugin`.

To stop `tedge-mapper-c8y`, `tedge-agent`, `tedge-mapper-az`, `tedge-mapper-aws`, you can simply run the commands below.

```sh
sudo tedge disconnect c8y
sudo tedge disconnect az
sudo tedge disconnect aws
```

To stop each component one by one, this is an example how to stop them with `systemctl`:

```sh
sudo systemctl stop tedge-mapper-c8y
sudo systemctl stop tedge-agent
sudo systemctl stop c8y-log-plugin
```
:::

Run `get-thin-edge_io.sh` script as below to upgrade to the latest version.

```sh
curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
```

:::note
If you want to upgrade to a specific version, append the `<version>` 3 digits, e.g. `0.7.3`:

```sh
curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s <version>
```
:::

## Manual installation

To install thin edge package it is required to use `curl` to download the package and `dpkg` to install it.

### Dependency installation

thin-edge.io has single dependency and it is `mosquitto` used for communication southbound and northbound e.g. southbound, devices can publish measurements; northbound, gateway may relay messages to cloud.
`mosquitto` can be installed with your package manager. For apt the command may look as following:

```sh
sudo apt install mosquitto
```

:::note
Some OSes may require you to use `sudo` to install packages.
:::

### Download package

thin-edge.io package is in thin-edge.io repository on GitHub: [thin-edge.io](https://github.com/thin-edge/thin-edge.io/releases).

To download the package from github repository use following command (use desired version):

```sh
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/<package>_<version>_<arch>.deb
```

Where:
- `version` -> thin-edge.io version in x.x.x format
- `arch` -> architecture type (amd64, armhf, arm64)

```sh title="Example"
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.9.0/tedge_0.9.0_armhf.deb
```

and for `mapper`:

```sh title="Example"
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.9.0/tedge-mapper_0.9.0_armhf.deb
```

### Install downloaded package

Now, we have downloaded the package we can proceed to installation. First we will install cli tool `tedge`.

:::note
Some OSes may require you to use `sudo` to install packages and therefore all following commands may need `sudo`.
:::

To install `tedge` use following command:

```sh
sudo dpkg -i tedge_<version>_<arch>.deb
```

```sh title="Example"
sudo dpkg -i tedge_0.5.0_armhf.deb
```

To install mapper for thin-edge.io do:

```sh
sudo dpkg -i tedge-mapper_<version>_<arch>.deb
```

```sh title="Example"
sudo dpkg -i tedge-mapper_0.9.0_armhf.deb
```

## Uninstall

The `thin-edge.io` can be uninstalled using a script, that can be downloaded
from below mentioned location.

Whether you are just removing the `thin-edge.io` packages or wanting to purge everything (removing the packages and configuration), there is a convenient one-liner provided under each section.

```sh
wget https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh
chmod a+x uninstall-thin-edge_io.sh
```

The uninstall script provides options as shown below.

```sh
USAGE: 
   ./uninstall-thin-edge_io.sh [COMMAND]
    
COMMANDS:
    remove     Uninstall thin-edge.io with keeping configuration files
    purge      Uninstall thin-edge.io and also remove configuration files
```

:::note
The uninstall script removes/purges the core thin-edge.io packages like `tedge,
tedge-mapper, and tedge-agent` as well as thin-edge.io plugins like `tedge-apt-plugin,
c8y-log-plugin, c8y-configuration-plugin` etc.
:::

### Remove

Use uninstall script as shown below just to `remove` the `thin-edge.io` packages.

```sh
sudo ./uninstall-thin-edge_io.sh remove
```

:::note
`./uninstall-thin-edge_io.sh remove` removes just the thin-edge.io packages and does not remove the `configuration` files.
:::

The same thing can also be executed using a one-liner to download and run the script.

```sh
curl -sSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh | sudo sh -s remove
```

### Purge

Use uninstall script as shown below to remove the thin-edge.io as well as to remove the `configuration` files that are
associated with these thin-edge.io packages.

```sh
sudo ./uninstall-thin-edge_io.sh purge
```

The same thing can also be executed using a one-liner to download and run the script.

```sh
curl -sSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh | sudo sh -s purge
```
