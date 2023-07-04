---
title: Enable Software Management Support
tags: [Operate, Installation, Software Management]
sidebar_position: 4
---

# Install and enable the software management feature

:::note
As of now, this feature is supported only on devices with **debian** based
distributions, which use the **apt** package manager(Ex: RaspberryPi OS , Ubuntu, Debian), from Cumulocity cloud.
:::

Below steps show how to download, install and enable thin-edge software management feature.

## Download and install software management packages on the device

As a prerequisite, install [tedge and tedge-mapper](../../install/index.md) if not installed already.

The thin-edge software management packages are in repository on GitHub: [thin-edge.io](https://github.com/thin-edge/thin-edge.io/releases).

To download the package from github repository use the following command (use desired version):

```sh
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/<package>_<version>_<arch>.deb
```

Where:
- `version` -> thin-edge.io software management components version in x.x.x format
- `arch` -> architecture type (amd64, armhf)

Download `tedge-apt-plugin` and `tedge-agent`

```sh title="Example"
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.9.0/tedge-apt-plugin_0.9.0_armhf.deb
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.9.0/tedge-agent_0.9.0_armhf.deb
```

Once the packages are downloaded, proceed to installation.

To install `tedge-apt-plugin` and `tedge-agent` on thin-edge device do:

```sh
sudo dpkg -i tedge-apt-plugin_<version>_<arch>.deb
sudo dpkg -i tedge-agent_<version>_<arch>.deb
```

:::note
Software management feature will be enabled after installation if the device
is connected to the Cumulocity cloud using:
```sh
sudo tedge connect c8y
```
:::

## Start and enable the software management feature

### Using tedge connect c8y

The `tedge connect c8y` will automatically start and enable the software management feature.
Find more about [how to connect thin-edge device to cloud](../connection/connect.md)

Once the thin-edge device is successfully connected to Cumulocity cloud, the **Software** option will be enabled and
the list of software that are installed on the device will be visible as shown in the figure below.

<p align="center">
    <img
        src={require('../../images/start-software-management.png').default}
        alt="Add new software"
        width="40%"
    />
</p>

:::note
Disconnecting thin-edge device from cloud with `tedge disconnect c8y` command will stop and disable the software management feature.
:::

### Manually enabling and disabling software management feature

For debugging purpose or to disable/enable the software management services, one can start/stop manually as shown below.

### Starting the services

```sh
sudo systemctl start tedge-agent
sudo systemctl start tedge-mapper-c8y
```

### Stopping the services

```sh
sudo systemctl stop tedge-agent
sudo systemctl stop tedge-mapper-c8y
```

## Filter packages by name and maintainer

You can filter the package list output using two filtering criteria: **name and maintainer**. Create the `apt` table in `tedge.toml` and fill it with the `name` and `maintainer` keys. The value of each filter key should be a **valid regex pattern**. Your filters should look like this:

```toml
[apt]
name = "exemplary_name.*"
maintainer  = "exemplary_maintainer"
```  

Also, filters can be provided as a command line parameter in the `apt-plugin`. However, they are created for testing purposes only and will override config parameters.
