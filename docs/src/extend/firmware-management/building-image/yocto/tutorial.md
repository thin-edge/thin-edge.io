---
title: Building an image
tags: [Extend, Build]
sidebar_position: 3
---

:::caution
Yocto is the defacto embedded linux distribution toolkit, however it is famous for its steep learning curve. Due to this it is **critical** that you follow the instructions, as deviating from them will mostly lead to problems!
:::

This guide walks you through a building a custom image using Yocto. The images produced by Yocto are always for dedicated devices as this allows the size of the images to significantly reduced as only the components which are related to the device are included. For example the images produced in this tutorial are only about 80MB in size. In comparison the "lite" image from Raspberry Pi is about 550MB!

Whilst the methodology described on this page can be extended to support other devices, these instructions focus in the following devices:

* Raspberry Pi 3 (64 bit)
* Raspberry Pi 4 (64 bit)

## Setting up your build system

Before being able to build your own image, you need to prepare an environment to run Yocto. Follow the instructions to get your build system setup.

1. Create a Virtual Machine running Ubuntu 20.04 (LTS)

    The virtual machine should be created 
    * Use a ubuntu 20.04 image (You can choose if you want a UI or not)
    * Disk with at least 100 GB, though more is better and generally it hard changing the size afterwards
    * Network enabled

2. Install essential packages

    ```sh tab={"label":"Ubuntu-20.04"}
    sudo apt install file gawk wget git diffstat unzip texinfo gcc build-essential chrpath socat cpio python3 python3-pip python3-pexpect xz-utils debianutils iputils-ping python3-git python3-jinja2 libegl1-mesa libsdl1.2-dev xterm python3-subunit mesa-common-dev zstd liblz4-tool
    ```

3. Install [git-lfs](https://git-lfs.com/) (this dependency will be removed in the next %%te%% release after 1.0.1)

    ```sh tab={"label":"Ubuntu-20.04"}
    curl -s https://packagecloud.io/install/repositories/github/git-lfs/script.deb.sh | sudo bash
    sudo apt-get install git-lfs
    ```

:::note
The large disk size might seem like a lot, but Yocto builds everything from scratch, and this takes time and disk space!
:::


## Building your image

The [meta-tedge-project](https://github.com/thin-edge/meta-tedge-project) is a meta project which uses [kas](https://kas.readthedocs.io/en/latest/) to package multiple Yocto (bitbake) projects into a single repository where the user can select which project they would like to build. It supports reusing configuration files to make it easier to configure different image definitions to control what should be included in your image.

Feel free to clone the project if you want to make your own customizations, however please always refer back to the project if you run into any problems (as it may have changed in the meantime).

The project uses different Yocto layers to build an image which contains:

|Item|Description|
|--|--|
|Yocto Linux Distribution|Base operating system|
|%%te%%|%%te%% and all of its components|
|%%te%% workflow|A workflow definition to perform Operation System A/B updates which interacts with the underlying technology, e.g. [RAUC](https://rauc.readthedocs.io/en/latest/) or [mender](https://github.com/mendersoftware/mender)|
|dependencies|Dependencies of the underlying firmware update mechanism, e.g. [RAUC](https://rauc.readthedocs.io/en/latest/) or [mender](https://github.com/mendersoftware/mender)|

### Cloning the project

1. Clone the project and change directory to the locally checked out folder

    ```sh
    git clone https://github.com/thin-edge/meta-tedge-project.git
    cd meta-tedge-project
    ```

2. Install [just](https://just.systems/man/en/chapter_5.html) which is used to run different tasks provided by the project

    ```sh
    curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | sudo bash -s -- --to /usr/bin
    ```

    Check out the [justfile website](https://just.systems/man/en/chapter_5.html) for more installation options.

3. Install [kas](https://kas.readthedocs.io/en/latest/) which is used to managed bitbake projects

    ```sh
    pip3 install kas
    ```

    :::note
    [kas](https://kas.readthedocs.io/en/latest/) is used to managed multiple Yocto bitbake projects.
    :::

### Configure shared folders

Using shared folders will help reduce the overall build times when building for different targets as the downloaded files can be reused.

1. Open a console in the project's root directory (e.g. inside the `tedge-meta-project` directory)

2. Create the following file with the given contents

    ```sh title="file: .env"
    SSTATE_DIR=/data/yocto/sstate-cache
    DL_DIR=/data/yocto/downloads
    ```

3. Create the shared folders which are referenced in the above `.env` file

    ```sh
    sudo mkdir -p /data/yocto/sstate-cache
    sudo chown -R "$(whoami):$(whoami)"  /data/yocto/sstate-cache

    sudo mkdir -p /data/yocto/downloads
    sudo chown -R "$(whoami):$(whoami)" /data/yocto/downloads
    ```


### Building an image

The following steps will build an image with %%te%% and [RAUC](https://rauc.readthedocs.io/en/latest/) installed to perform firmware (OS) updates.

1. Build an image for your device (machine)

    ```sh tab={"label":"RaspberryPi-3"}
    KAS_MACHINE=raspberrypi3-64 just build-project ./projects/tedge-rauc.yaml
    ```

    ```sh tab={"label":"RaspberryPi-4"}
    KAS_MACHINE=raspberrypi4-64 just build-project ./projects/tedge-rauc.yaml
    ```

    The `KAS_MACHINE` is used to select the target device.

    :::note
    Yocto build take a long time (&gt; 4 hours) to build so be patient. A build time of 12 hours is also not unheard of!
    :::

2. Inspect the built image

    ```sh
    ls -l tmp
    ```

3. Flash the base image using the instructions on the [Flashing an image](../../flashing-an-image.md) page

:::tip
Check out the project files under the `./projects` directory for other available project examples. If you don't find a project that fits your needs, then just create your own project definition.
:::

:::note
The **tedge-rauc** project includes a package manager (apt) so that additional packages can be installed without having to do a full image update. If you don't want the image to include a package manager, you can customize the project yourself and remove the related configuration file.
:::

## Tips

This section contains general tips which can be helpful whilst either getting things setup, or what to do when you encounter an error.

### Building on MacOS Apple Silicon

If you are trying to build on a MacOS Apple Silicon computer, then you will have to create a Virtual Machine (e.g. [UTM](https://mac.getutm.app/), [lima](https://github.com/lima-vm/lima) or equivalents).

* Ubuntu 20.04 (headless variant is enough)
* Don't use CPU emulation, use the native `aarch64` option, otherwise your builds will be even slower!

### Unexpected/cryptic build errors

Whilst building device images it is common that at some people you will run into a build error. These errors can be very cryptic and hard to find the root cause. Luckily, more often than not, these errors are caused due to some corrupted state within the temporary build output folder.

So before creating a ticket, make sure you clean the folder using the following command:

```sh
just clean
```

Then rebuild the project that you originally tried to build.
