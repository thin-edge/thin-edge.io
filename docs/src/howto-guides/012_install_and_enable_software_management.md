# Install and enable the software management feature

```admonish note
As of now, this feature is supported only on devices with **debian** based
distributions, which use the **apt** package manager(Ex: RaspberryPi OS , Ubuntu, Debian), from Cumulocity cloud.
```

Below steps show how to download, install and enable thin-edge software management feature.

## Download and install software management packages on the device

As a prerequisite, install [tedge and tedge-mapper](../howto-guides/002_installation.md) if not installed already.

The thin-edge software management packages are in repository on GitHub: [thin-edge.io](https://github.com/thin-edge/thin-edge.io/releases).

To download the package from github repository use the following command (use desired version):

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/<package>_<version>_<arch>.deb
```

where:
- `version` -> thin-edge.io software management components version in x.x.x format
- `arch` -> architecture type (amd64, armhf)

Download `tedge-apt-plugin` and `tedge-agent`

```shell
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.9.0/tedge-apt-plugin_0.9.0_armhf.deb
curl -LJO https://github.com/thin-edge/thin-edge.io/releases/download/0.9.0/tedge-agent_0.9.0_armhf.deb
```

Once the packages are downloaded, proceed to installation.

To install `tedge-apt-plugin` and `tedge-agent` on thin-edge device do:

```shell
sudo dpkg -i tedge-apt-plugin_<version>_<arch>.deb
sudo dpkg -i tedge-agent_<version>_<arch>.deb
```

```admonish note
Software management feature will be enabled after installation if the device
is connected to the Cumulocity cloud using `tedge connect c8y`.
```

## Start and enable the software management feature

### Using `tedge connect c8y`

The `tedge connect c8y` will automatically start and enable the software management feature.
Find more about [how to connect thin-edge device to cloud](../howto-guides/004_connect.md)

Once the thin-edge device is successfully connected to Cumulocity cloud, the **Software** option will be enabled and
the list of software that are installed on the device will be visible as shown in the figure below.

![Add new software](./images/start-software-management.png)

```admonish note
Disconnecting thin-edge device from cloud with `tedge disconnect c8y` command will stop and disable the software management feature.
```

### Manually enabling and disabling software management feature

For debugging purpose or to disable/enable the software management services, one can start/stop manually as shown below.

### Starting the services

```shell
sudo systemctl start tedge-agent.service
sudo systemctl start tedge-mapper-c8y.service
```

### Stopping the services

```shell
sudo systemctl stop tedge-agent.service
sudo systemctl stop tedge-mapper-c8y.service
```

