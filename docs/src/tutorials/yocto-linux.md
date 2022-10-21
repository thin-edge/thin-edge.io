# Build Thin Edge for a Yocto Linux distribution

Yocto Project enables you to create a customised Linux distribution for your IoT devices. You can select the base image
and add layers, containing software that you need on your image. In this tutorial, we will add Thin Edge using the
`meta-tedge` layer. For more information, see the [getting started document on Yocto Project
website](https://www.yoctoproject.org/software-overview/).


`meta-tedge` is supported for **Yocto version 3.4 "Honister" and 4.0 "Kirkstone"**. Additionally, we will depend on `meta-openembedded`
layer for Mosquitto.

## Installation

If you are not familiar with building Yocto distribution or you have not configured your build host yet, we strongly
recommend to look into [official yocto documentation](https://docs.yoctoproject.org/brief-yoctoprojectqs/index.html)
as the installation process will now skip all information that were mentioned there! For workspace organization or
raspberry pi distribution, we also recommend this [guide](https://github.com/jynik/ready-set-yocto)

> Most of the installation process is based on [Yocto Project Quick Build
guide](https://docs.yoctoproject.org/brief-yoctoprojectqs/index.html).

Before starting, be sure the host, where you plan to build a new Yocto image with thin-edge, meets the following
requirements:

- 50 Gbytes of free disk space
- a Linux distributions that supports the Yocto Project, see the [Supported Linux
Distributions](https://docs.yoctoproject.org/4.0.4/ref-manual/system-requirements.html#supported-linux-distributions)
section in the Yocto Project Reference Manual. For detailed information on preparing your build host, see the [Preparing
the Build Host](https://docs.yoctoproject.org/4.0.4/dev-manual/start.html#preparing-the-build-host) section in the Yocto
Project Development Tasks Manual.
- Git 1.8.3.1 or greater
- tar 1.28 or greater
- Python 3.6.0 or greater.
- gcc 5.0 or greater.

For the purposes of the tutorial, we assume `~/yocto-thinedge` working directory. If using other directory, be sure to
change paths where needed.

### Build Host Packages

Install essential packages:

```bash
$ sudo apt install gawk wget git diffstat unzip texinfo gcc build-essential chrpath socat cpio python3 python3-pip python3-pexpect xz-utils debianutils iputils-ping python3-git python3-jinja2 libegl1-mesa libsdl1.2-dev pylint3 xterm python3-subunit mesa-common-dev zstd liblz4-tool
```

### Clone Poky Linux repository

Clone the Poky Linux distribution sources. We'll be using version `kirkstone`. You can use `--depth=1` to speed up the
process. If using these options, to see previous commits or other branches see
[here](https://stackoverflow.com/questions/29270058/how-to-fetch-all-git-history-after-i-clone-the-repo-with-depth-1)
and [here](https://stackoverflow.com/questions/17714159/how-do-i-undo-a-single-branch-clone).

```
$ git clone git://git.yoctoproject.org/poky --branch=kirkstone --depth=1
```

### Add layers

We'll be using `meta-tedge` and `meta-openembedded`. First, fetch the repositories:

```bash
git clone https://github.com/thin-edge/meta-tedge
git clone git://git.openembedded.org/meta-openembedded
```

Next, add the following layers:

```bash
bitbake-layers add-layer ~/yocto-thinedge/meta-openembedded/meta-oe
bitbake-layers add-layer ~/yocto-thinedge/meta-openembedded/meta-python
bitbake-layers add-layer ~/yocto-thinedge/meta-openembedded/meta-networking
bitbake-layers add-layer ~/yocto-thinedge/meta-tedge

```

### Configure the build

Activate `Systemd` as default init manager by adding following line to `local.conf`:

```
INIT_MANAGER="systemd"
```

Build `tedge` by running following command :

```bash
$ bitbake tedge-full-image
```

### Run the build

When the build it complete, run it with qemu:

```bash
$ runqemu nographic
```


## Further recommendations

After building the reference distribution and image, you can explore creating your own layer and image, and then
integrating `tedge-*` recipes for it. 

### Configure and run the layer on Raspberry Pi device

> Note: following installation process will base on tutorial from the previous chapter

After successful run in qemu, we can run it on raspberry pi by adjusting our build to a proper architecture.

To do that, we will use `meta-raspberrypi` layer that we need to fetch and `meta-openembedded` that we fetched previously:

```bash
git clone -b kirkstone https://github.com/agherzan/meta-raspberrypi.git
```

According to the `meta-raspberrypi/README.md`, we have all the dependencies added to the layer except `meta-multimedia` that we need to add with `add-layer` subcommand. After that, we can add `meta-raspberrypi` itself:

```bash
bitbake-layers add-layer ~/yocto-thinedge/meta-openembedded/meta-networking
bitbake-layers add-layer ~/yocto-thinedge/meta-raspberrypi
```

Next, we open up `conf/local.conf` and find this line:

```bash
MACHINE ??= "qemux86-64"
```

It denotes which platform we are targeting. Select the one that fits that platform you'd like to build an image for. All available platforms can be found in `meta-raspberrypi/machine/` directory. In our case, we target Raspberry Pi 3 in 64-bit mode:

```bash
MACHINE = "raspberrypi3-64"
```

We can also change the specific configuration of the Raspberry Pi machine. In `meta-raspberrypi/docs/extra-build-config.md` we can find a variety of `local.conf` definitions that you can use to enable/disable/modify functionality of a device, e.g to access a shell via the UART, add following line to `local.conf` file:

```bash
ENABLE_UART = "1"
```

After we finish the configuration, we can build an image using `tedge-full-image`:

```bash
$ bitbake tedge-full-image
```

Once the build is complete, the image will be located in `/tmp/deploy/images/$MACHINE/` directory where `$MACHINE` denotes your target platform. Copy the image to the SD card and run your device. 

To make Yocto run on another hardware, check other layers in the [OpenEmbedded Layer Index](https://layers.openembedded.org/layerindex/branch/master/layers/).