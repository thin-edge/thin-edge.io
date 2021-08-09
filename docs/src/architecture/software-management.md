# Software Management with thin-edge.io

With thin-edge.io you can ease the burden of managing packages on your device.
Software Management operates end to end from a cloud down to the OS of your device and reports statuses accordingly.

## Software management components

Software Management uses following components to perform software operations:

### Cloud Mapper

TBD

### Tedge Agent

TBD

### Software Manager Plugin

TBD

## Installation

### tegde_agent

`tedge_agent` is distributed as debian package and can be installed with following command:

```shell
sudo dpkg -i tedge_agent
```

The installation will add `systemd` service `tedge-agent.service` and new user specific to the agent (`tedge-agent`).
As some of the operations may require `root` permissions or `sudo` access it is advised that the tedge-agent user is added to the `sudo` group which will allow it to execute elevated commands.

You can do with following command:

```shell
sudo usermod -aG sudo tedge-agent
```

To start the agent you can do:

```shell
sudo systemctl restart tedge-agent
```

#### Plugins

SM Plugins should be stored in `/etc/tedge/sm-plugins` directory.
