---
title: Init Systems
tags: [Installation, Unix, Init, Services]
sidebar_position: 1
---

# Init systems

thin-edge.io supports Systemd out of the box, however not all Linux distributions use Systemd. To use thin-edge.io on a Linux distribution without Systemd requires a few extra steps.

Support for different init systems (service managers) is provided by a community repository, [tedge-services](https://github.com/thin-edge/tedge-services). The following service definitions are currently supported (though check the community repository if you don't see your preferred init system in the list).

* OpenRC
* runit
* s6-overlay
* SysVinit
* supervisord

You are also free to use any service manager to run thin-edge how you want. Check out the [init system reference](../../references/init-system-config.md) guide to see how to create a configuration to interact with your preferred init system.

:::tip
Contributions are welcome in the [tedge-services](https://github.com/thin-edge/tedge-services) repository to improve any of the services, or to add support for additional init systems.
:::

## Install

You can install the service definitions using a convenient script which will auto detect the init system for you.

```sh tab={"label":"curl"}
curl -fsSL https://thin-edge.io/install-services.sh | sh -s
```

```sh tab={"label":"wget"}
wget -O - https://thin-edge.io/install-services.sh | sh -s
```

However, if you know which init system you are using on your device, and would like to manually specify it, then you can use the following command:

```sh tab={"label":"curl"}
curl -fsSL https://thin-edge.io/install-services.sh | sh -s -- supervisord
```

```sh tab={"label":"wget"}
wget -O - https://thin-edge.io/install-services.sh | sh -s -- supervisord
```

## Alternative installation methods

In cases were you would not like to run the automatic install script, you can choose one to run the steps manually. This allows you more control over the process which can be useful if you are experiencing problems with the auto detection used in the install script.

### Manual repository setup and installation

The software repositories used by the package managers can be configured using the setup scripts. These scripts are normally executed by the *install-services.sh* script in the installation section, however they can also be manually executed if you want more fine-grain control over the process.

:::tip
If you are having problems setting any of the repositories, check out the [Cloudsmith](https://cloudsmith.io/~thinedge/repos/community/setup/#formats-deb) website where they have **Set Me Up** instructions in additional formats, e.g. manual configuration rather than via the `setup.*.sh` script.
:::

**Pre-requisites**

The instructions require you to have the following tools installed.

* bash
* curl

#### Setup

**Running with sudo**

You will need to have `sudo` also installed if you want to run these instructions.

```sh tab={"label":"Debian/Ubuntu"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.deb.sh' | sudo bash
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.rpm.sh' | sudo bash
```

```sh tab={"label":"Alpine"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.alpine.sh' | sudo bash
```

**Running as root**

These commands must be run as the root user.

```sh tab={"label":"Debian/Ubuntu"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.deb.sh' | bash
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.rpm.sh' | bash
```

```sh tab={"label":"Alpine"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.alpine.sh' | bash
```

### Installing and updating using a package manager

Once you have the repository setup, you can install the service definitions for your preferred init system.

#### OpenRC

```sh tab={"label":"Debian/Ubuntu"}
sudo apt-get install tedge-openrc
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
sudo dnf install tedge-openrc
```

```sh tab={"label":"Alpine"}
sudo apk add tedge-openrc
```

#### runit

```sh tab={"label":"Debian/Ubuntu"}
sudo apt-get install tedge-runit
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
sudo dnf install tedge-runit
```

```sh tab={"label":"Alpine"}
sudo apk add tedge-runit
```

#### s6-overlay

```sh tab={"label":"Debian/Ubuntu"}
sudo apt-get install tedge-s6overlay
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
sudo dnf install tedge-s6overlay
```

```sh tab={"label":"Alpine"}
sudo apk add tedge-s6overlay
```

#### SysVinit

```sh tab={"label":"Debian/Ubuntu"}
sudo apt-get install tedge-sysvinit
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
sudo dnf install tedge-sysvinit
```

```sh tab={"label":"Alpine"}
sudo apk add tedge-sysvinit
```

#### supervisord

```sh tab={"label":"Debian/Ubuntu"}
sudo apt-get install tedge-supervisord
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
sudo dnf install tedge-supervisord
```

```sh tab={"label":"Alpine"}
sudo apk add tedge-supervisord
```

After installing the supervisord definitions, you will have to make sure the supervisord configuration pulls in the services definitions. Below shows an example `supervisord.conf` file which imports all thin-edge.io services definitions which were installed.

```ini title="file: /etc/supervisord.conf"
# ... other supervisord settings

[include]
files = /etc/supervisor/conf.d/*.conf
```

### Install via tarball

You can force the install-services.sh script to install via the tarball instead of via a package manager.

To install the service definitions via the tarball run the following command:

```sh tab={"label":"curl"}
curl -fsSL https://thin-edge.io/install-services.sh | sh -s -- --package-manager tarball
```

```sh tab={"label":"wget"}
wget -O - https://thin-edge.io/install-services.sh | sh -s -- --package-manager tarball
```

Or if you also want to manually specify the init system to install, then you can the following command:

```sh tab={"label":"curl"}
curl -fsSL https://thin-edge.io/install-services.sh | sh -s -- supervisord --package-manager tarball
```

```sh tab={"label":"wget"}
wget -O - https://thin-edge.io/install-services.sh | sh -s -- supervisord --package-manager tarball
```
