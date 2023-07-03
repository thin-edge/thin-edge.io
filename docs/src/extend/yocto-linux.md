---
title: Thin-edge on Yocto
tags: [Extend, Build]
sidebar_position: 2
---

# Build Thin Edge for a Yocto Linux distribution

Yocto Project enables you to create a customised Linux distribution for your IoT devices. You can select the base image
and add layers, containing software that you need on your image. In this tutorial, we will add Thin Edge using the
`meta-tedge` layer. For more information, see the [getting started document on Yocto Project
website](https://www.yoctoproject.org/software-overview/).

The `meta-tedge` is supported for **Yocto version 3.4 "Honister" and 4.0 "Kirkstone"**. It depends on `meta-networking`, `meta-python` and `meta-oe` layers, which are part of `meta-openembedded` layer. Since version 0.9.0, the layer requires `meta-rust` to meet the requirements of the rust version of thin-edge. 

## Installation

If you are not familiar with building Yocto distribution or you have not configured your build host yet, we strongly
recommend to look into [official yocto documentation](https://docs.yoctoproject.org/brief-yoctoprojectqs/index.html)
as the installation process will now skip all information that were mentioned there! For workspace organization or
raspberry pi distribution, we also recommend this [guide](https://github.com/jynik/ready-set-yocto)

:::note
Most of the installation process is based on
[Yocto Project Quick Build guide](https://docs.yoctoproject.org/brief-yoctoprojectqs/index.html).
:::

Before starting, be sure the host, where you plan to build a new Yocto image with thin-edge, meets the following
requirements:

- 50 GB of free disk space
- a Linux distributions that supports the Yocto Project, see the [Supported Linux
Distributions](https://docs.yoctoproject.org/4.0.4/ref-manual/system-requirements.html#supported-linux-distributions)
section in the Yocto Project Reference Manual. For detailed information on preparing your build host, see the [Preparing
the Build Host](https://docs.yoctoproject.org/4.0.4/dev-manual/start.html#preparing-the-build-host) section in the Yocto
Project Development Tasks Manual.
- Git 1.8.3.1 or greater
- tar 1.28 or greater
- Python 3.6.0 or greater.
- gcc 5.0 or greater.

:::note
For the purposes of the tutorial, we assume `/home/yocto/` working directory. If using other directory, be sure to
change paths where needed.
:::

### Build Host Packages

Install essential packages:

```sh
sudo apt install gawk wget git diffstat unzip texinfo gcc build-essential chrpath socat cpio python3 python3-pip python3-pexpect xz-utils debianutils iputils-ping python3-git python3-jinja2 libegl1-mesa libsdl1.2-dev pylint3 xterm python3-subunit mesa-common-dev zstd liblz4-tool
```

### Clone Poky Linux repository

Clone the Poky Linux distribution sources. We'll be using version `kirkstone`. You can use `--depth=1` to speed up the
process. If using these options, to see previous commits or other branches see
[here](https://stackoverflow.com/questions/29270058/how-to-fetch-all-git-history-after-i-clone-the-repo-with-depth-1)
and [here](https://stackoverflow.com/questions/17714159/how-do-i-undo-a-single-branch-clone).

Alternatively, you could use `--branch=honister` for Yocto version 3.4 Honister.
If doing so, remember to also use `--branch=honister` for all additional layers that require it.

```sh
git clone git://git.yoctoproject.org/poky --branch=kirkstone --depth=1
```

Resulting directory structure should look like this:

```
.
└── poky
    ├── bitbake
    ├── contrib
    ├── documentation
    ├── LICENSE
    ├── LICENSE.GPL-2.0-only
    ├── LICENSE.MIT
    ├── MAINTAINERS.md
    ├── Makefile
    ├── MEMORIAM
    ├── meta
    ├── meta-poky
    ├── meta-selftest
    ├── meta-skeleton
    ├── meta-yocto-bsp
    ├── oe-init-build-env
    ├── README.hardware.md -> meta-yocto-bsp/README.hardware.md
    ├── README.md -> README.poky.md
    ├── README.OE-Core.md
    ├── README.poky.md -> meta-poky/README.poky.md
    ├── README.qemu.md
    └── scripts
```

### Initialize the build environment

To use the `bitbake`, `bitbake-layers`, `runqemu`, or other tools used in yocto to create or run our build, we need to
source the `poky/oe-init-build-env` script in our shell:

```
cd poky
source oe-init-build-env
```

The script creates a `build` directory, which itself contains only a `conf` directory:

```
.
└── poky
    ├── build
    │   └── conf
    │       ├── bblayers.conf
    │       ├── local.conf
    │       └── templateconf.cfg
    ...
```

The script prepares the environment such that we can run `bitbake` or `runqemu` in any directory, but `bitbake-layers`
command must be run from `build` directory.

Inside the `poky/build/conf` directory, there are 2 files of interest to us:

- `bblayers.conf` contains all the layers used by build
- `local.conf` contains local build configuration, ie. configuration that applies only to the build

In the following steps, we will edit these files to customize our image.

### Add layers

`oe-init-build-env` moved us to the `build` directory. `cd` back to the working directory and clone the `meta-tedge`, `meta-rust` and
`meta-openembedded` repositories:

```sh
cd ../../
git clone https://github.com/thin-edge/meta-tedge
git clone https://github.com/meta-rust/meta-rust.git
git clone --branch=kirkstone git://git.openembedded.org/meta-openembedded
```

:::note
As with Poky itself, when cloning meta-openembedded either provide `--branch=kirkstone` for the clone command or
manually `git switch kirkstone` in `meta-openembedded` repository after cloning. Some Yocto layers support different
Yocto versions on different branches, in which case be sure to select correct branch. `meta-tedge` however supports 2
versions, Honister and Kirkstone on the main branch, so there's no need to change the default branch.
:::

Resulting in the following directory structure:

```
.
├── meta-openembedded
├── meta-rust
├── meta-tedge
└── poky
```

As these layers and `poky` are all different git repositories, we clone them next to the poky directory, but you can
put them inside `poky` if you prefer. The only thing that matters is to use a correct path in the
`poky/build/conf/bblayers.conf` file.

Next, we add these layers to our build using `bitbake-layers` tool. Be aware that `meta-openembedded` is not itself a
layer, but a collection of many layers. We need to run it from the `build` directory:

```sh
cd poky/build
bitbake-layers add-layer /home/yocto/meta-openembedded/meta-oe
bitbake-layers add-layer /home/yocto/meta-openembedded/meta-python
bitbake-layers add-layer /home/yocto/meta-openembedded/meta-networking
bitbake-layers add-layer /home/yocto/meta-rust
bitbake-layers add-layer /home/yocto/meta-tedge
```

`bitbake-layers` tool adds the layer to our build by modyfying `poky/build/conf/bblayers.conf` file. You can verify the
file contains the above layers, or if something is wrong with the tool, you can edit the file manually, adding the
correct paths. The use of absolute paths is required.

```text title="file: poky/build/conf/bblayers.conf"
# POKY_BBLAYERS_CONF_VERSION is increased each time build/conf/bblayers.conf
# changes incompatibly
POKY_BBLAYERS_CONF_VERSION = "2"

BBPATH = "${TOPDIR}"
BBFILES ?= ""

BBLAYERS ?= " \
  /home/yocto/poky/meta \
  /home/yocto/poky/meta-poky \
  /home/yocto/poky/meta-yocto-bsp \
  /home/yocto/meta-openembedded/meta-oe \
  /home/yocto/meta-openembedded/meta-python \
  /home/yocto/meta-openembedded/meta-networking \
  /home/yocto/meta-rust \
  /home/yocto/meta-tedge \
  "
```

### Use systemd init manager

For thin-edge to work, it has to run some setup scripts upon the first boot and interact with system services via the
init system. Right now, `meta-tedge` only supports systemd init manager. The default in yocto is initrd, thus we have to
change it.

:::caution
This, and any successive changes to the build configuration should happen only in your local configuration, ie.
`poky/build/conf/local.conf` or in configuration files of layers you create yourself. Changing any files in layers you
do not control - in our example `meta-tedge` or any of the layers in `meta-openembedded` - is discouraged, because any
changes you make to them may be lost when you update the layer.
:::

Activate `systemd` as default init manager by adding following line to `poky/build/conf/local.conf`:

```text title="file: poky/build/conf/local.conf"
INIT_MANAGER="systemd"
```

### Use apt package manager (optional)

In Yocto one can enable a package manager for installing or removing packages during runtime. To
include a package manager in our build, we need to add `package-management` feature:

In `poky/build/conf/local.conf`, add the following line:

```text title="file: poky/build/conf/local.conf"
EXTRA_IMAGE_FEATURES += "package-management"
```

Additionally, we can choose between 3 available package formats, and their associated package managers.
Let us use `deb` package format and the apt package manager:

In `poky/build/conf/local.conf` find the following section and change `PACKAGE_CLASSES` from `package_rpm` to
`package_deb` as such:

```text title="file: poky/build/conf/local.conf"
#
# Package Management configuration
#
# This variable lists which packaging formats to enable. Multiple package backends
# can be enabled at once and the first item listed in the variable will be used
# to generate the root filesystems.
# Options are:
#  - 'package_deb' for debian style deb files
#  - 'package_ipk' for ipk files are used by opkg (a debian style embedded package manager)
#  - 'package_rpm' for rpm style packages
# E.g.: PACKAGE_CLASSES ?= "package_rpm package_deb package_ipk"
# We default to rpm:
PACKAGE_CLASSES ?= "package_deb"
```

### Build and run

Finally, build the image and run it in the emulator.

Build `core-image-tedge` by running following command, from any directory:

```sh
bitbake core-image-tedge
```

Bitbake tool will begin the building process, and all build artifacts will be put in `poky/build/tmp` directory.

When the build it complete, run it with qemu from any directory. It will automatically run the latest image built. We'll
use `nographic` option to run the emulator inside the current terminal, so that we may copy-paste from our system
clipboard, which wouldn't work in an external window. If this is not necessary, or if you run a graphical image, such as
`core-image-sato`, you can omit this option.

:::tip
For more information about runqemu tool, see [Using the Quick EMUlator
(QEMU)](https://docs.yoctoproject.org/4.0.5/dev-manual/qemu.html) in Yocto Development Tasks manual.
:::

```sh
runqemu nographic
```

After booting up, there will be a prompt to login. Login as `root`, password won't be required for the default
configuration.

### Configure and run the layer on Raspberry Pi device

After successful run in qemu, we can run it on raspberry pi by adjusting our build to a proper architecture.

To do that, we will use `meta-raspberrypi` layer that we need to fetch and `meta-openembedded` that we fetched
previously:

```sh
git clone -b kirkstone https://github.com/agherzan/meta-raspberrypi.git
```

According to the `meta-raspberrypi/README.md`, we have all the dependencies added to the layer except `meta-multimedia`
that we need to add with `add-layer` subcommand. After that, we can add `meta-raspberrypi` itself:

```sh
bitbake-layers add-layer /home/yocto/meta-openembedded/meta-multimedia
bitbake-layers add-layer /home/yocto/meta-raspberrypi
```

Next, we open up `poky/build/conf/local.conf` and find this line:

```text title="file: poky/build/conf/local.conf"
MACHINE ??= "qemux86-64"
```

It denotes which platform we are targeting. Select the one that fits that platform you'd like to build an image for. All
available platforms can be found in `meta-raspberrypi/machine/` directory. In our case, we target Raspberry Pi 3 in
64-bit mode:

```
MACHINE = "raspberrypi3-64"
```

We can also change the specific configuration of the Raspberry Pi machine. In
`meta-raspberrypi/docs/extra-build-config.md` we can find a variety of `local.conf` definitions that you can use to
enable/disable/modify functionality of a device, e.g to access a shell via the UART, add following line to
`poky/build/conf/local.conf` file:

```text title="file: poky/build/conf/local.conf"
ENABLE_UART = "1"
```

After we finish the configuration, we can build an image using `core-image-tedge`:

```sh
bitbake core-image-tedge
```

Once the build is complete, the image will be located in `/tmp/deploy/images/$MACHINE/` directory where `$MACHINE`
denotes your target platform. Copy the image to the SD card and run your device.

:::tip
To make Yocto run on another hardware, check other layers in the
[OpenEmbedded Layer Index](https://layers.openembedded.org/layerindex/branch/master/layers/).
:::

## Further recommendations

After building the reference distribution and image, you can explore creating your own layer and image, and then
integrating `tedge-*` recipes for it.
