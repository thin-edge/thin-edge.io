---
title: Install on Linux
tags: [Installation, Unix]
sidebar_position: 2
---

# Install on other Linux platforms

## Installation on debian-based platforms

`thin-edge.io` can be installed on a range of platforms.
A platform is defined as a set of hardware architecture and OS.
More details can be found in [Supported Platforms](../../references/supported-platforms.md) document.

Out of the box `thin-edge.io` uses deb packages for an automated installation ([Installation Guide](../../install/index.md)).
You can install it yourself on any Linux system as long as you follow the guidelines below.

## Installation on other platforms

### Obtaining binaries

The prebuilt binaries can be obtained from `thin-edge.io` [repository releases](https://github.com/thin-edge/thin-edge.io/releases).

By default `thin-edge.io` is built with 3 architectures in mind:
`amd64 (x86_64)`, `arm64 (aarch64)` and `armhf` with gnulibc bindings.
So if you are looking to install `thin-edge.io` on a different platform
you have to build your own binaries from source
which you can do easily if you follow the [Building `thin-edge.io`](../../contribute/BUILDING.md) guide.

:::note
By default `thin-edge.io` is built with `musl`, but it is possible to use `GNU libc` instead.
:::

Full installation of `thin-edge.io` requires the following components:

* tedge
* tedge-mapper
* tedge-agent

#### Extracting binaries from deb packages

Required packages:
* ar
* tar

Currently all binaries provided with releases are packaged into `deb` packages.
`deb` packages can be extracted to get the binaries for installation (example):

```sh title="Example"
ar -x tedge_<version>_amd64.deb | tar -xf data.tar.xz
ar -x tedge-agent_<version>_amd64.deb | tar -xf data.tar.xz
ar -x tedge-mapper_<version>_amd64.deb | tar -xf data.tar.xz
```

Which should give you `usr` and/or `lib` directory where you can find binaries.
After extracting all packages, you should now adjust permissions on those files:

```sh
chown root:root /usr/bin/tedge
chown root:root /usr/bin/tedge-agent
chown root:root /usr/bin/tedge-mapper
```

and then move your binaries to the appropriate directory, eg:

```sh
mv ./lib/ ./bin/ /
```

#### If building from source

If you have built the binaries from source you should install them on the target in: `/usr/bin/`.

`systemd` unit files for `tedge-mapper` and `tedge-agent` can be found in the repository at `configuration/init/systemd/tedge-*` and should be installed on the target in: `lib/systemd/system/tedge-*`.

### Configuring the system and systemd-units

`thin-edge.io` relies on certain system configuration and systemd process management, when installing from deb package all of that is setup automatically but with manual installation a set of steps has to be performed.

On most Linux distribution it should suffice to execute them as `root` to do the setup, but in some cases (eg, your system uses `useradd` instead of `adduser` package) more detailed instructions are documented:

* [tedge](https://github.com/thin-edge/thin-edge.io/blob/main/configuration/debian/tedge/postinst)
* [tedge-agent](https://github.com/thin-edge/thin-edge.io/blob/main/configuration/debian/tedge-agent/postinst)
* [tedge-mapper](https://github.com/thin-edge/thin-edge.io/blob/main/configuration/debian/tedge-mapper/postinst)

After following steps for all the components installed `thin-edge.io` should be operational.
