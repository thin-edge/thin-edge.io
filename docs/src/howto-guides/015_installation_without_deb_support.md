# How to install `thin-edge.io` on any Linux OS (no deb support)?

## `thin-edge.io` on supported platforms

`thin-edge.io` can be installed on a range of platforms, a platform is defined as a set of hardware architecture and OS, more details can be found in [Supported Platforms](../supported-platforms.md) document.

Out of the box `thin-edge.io` uses deb packages for an automated installation ([Installation Guide](./002_installation.md)), you can install it yourself on any Linux system as long as you follow the guidelines below.

## Installation on 'unsupported platforms'

### Obtaining binaries

The prebuilt binaries can be obtained from `thin-edge.io` [repository releases](https://github.com/thin-edge/thin-edge.io/releases).

By default `thin-edge.io` is built with 3 architectures in mind: `amd64 (x86_64)`, `arm64 (aarch64)` and `armhf` with gnulibc bindings, so if you are looking to install `thin-edge.io` on a different platform you have to build your own binaries from source which you can do easily if you follow the [Building `thin-edge.io`](./../BUILDING.md) guide.

> Note: By default `thin-edge.io` is built with `GNU libc`, but it is possible to use `musl` instead.

Full installation of `thin-edge.io` requires the following components:

* tedge
* tedge-mapper
* tedge-agent

#### Extracting binaries from deb packages

> Required packages:
>
> * ar
> * tar

Currently all binaries provided with releases are packaged into `deb` packages.
`deb` packages can be extracted to get the binaries for installation (example):

```shell
ar -x tedge_<version>_amd64.deb | tar -xf data.tar.xz
ar -x tedge_agent_<version>_amd64.deb | tar -xf data.tar.xz
ar -x tedge_mapper_<version>_amd64.deb | tar -xf data.tar.xz
```

Which should give you `usr` and/or `lib` directory where you can find binaries.
After extracting all packages, you should now adjust permissions on those files:

```shell
chown root:root /usr/bin/tedge
chown root:root /usr/bin/tedge_agent
chown root:root /usr/bin/tedge_mapper
```

and then move your binaries to the appropriate directory, eg:

```shell
mv ./lib/ ./bin/ /
```

#### If building from source

If you have built the binaries from source you should install them on the target in: `/usr/bin/`.

`systemd` unit files for `tedge_mapper` and `tedge_agent` can be found in the repository at `configuration/init/systemd/tedge-*` and should be installed on the target in: `lib/systemd/system/tedge-*`.

### Configuring the system and systemd-units

`thin-edge.io` relies on certain system configuration and systemd process management, when installing from deb package all of that is setup automatically but with manual installation a set of steps has to be performed.

On most Linux distribution it should suffice to execute them as `root` to do the setup, but in some cases (eg, your system uses `useradd` instead of `adduser` package) more detailed instructions are documented in the script files, you can find them under "/configuration/debian/" and subfolders.

After following steps for all the components installed `thin-edge.io` should be operational.
