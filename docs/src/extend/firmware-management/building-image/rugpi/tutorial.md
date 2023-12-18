---
title: Building an image
tags: [Extend, Build]
sidebar_position: 3
---

This guide walks you through building a custom image using [Rugpi](https://github.com/silitics/rugpi). Rugpi is an open source tool to build images with support for robust Over-the-Air (OTA) updates. Currently it only supports Raspberry Pi devices, however in the future more device types may be considered.

Rugpi is very beginner friendly compared to The Yocto Project, and has much shorter build times since it works by extending the official Raspberry Pi images with custom recipes which either install or configure components in the image, rather than rebuilding everything from scratch. Building an image with custom recipes only takes about 10 mins compared with the > 4 hours using The Yocto Project.

The instructions on this page can be used to build images for the following devices:

* Raspberry Pi 1 (32 bit)
* Raspberry Pi 2 (64 bit)
* Raspberry Pi 3 (64 bit)
* Raspberry Pi 4 (64 bit)
* Raspberry Pi 5 (64 bit)
* Raspberry Pi Zero W (32 bit)
* Raspberry Pi Zero 2W (64 bit)

Please check out the [official Rugpi documentation](https://oss.silitics.com/rugpi/) for more details on how to use and customize it.

## Setting up your build system

Building images using Rugpi is considerably faster and easier when compared to The Yocto Project, so it makes it a great tool for beginners.

The following tools are required to build an image:

* Docker Engine

:::tip
Images can be built using Rugpi using a CI Workflow. An example for a Github Workflow is included in the template project.
:::

## Building your image

The [tedge-rugpi-images](https://github.com/thin-edge/tedge-rugpi-image) is project which includes out of the box configuration to perform robust Over-the-Air Operating System updates.

Feel free to clone the project if you want to make your own customizations, however please always refer back to the project if you run into any problems (as it may have changed in the meantime).


### Cloning the project

1. Clone the project and change directory to the locally checked out folder

    ```sh
    git clone https://github.com/thin-edge/tedge-rugpi-image.git
    cd tedge-rugpi-image
    ```

2. Install [just](https://just.systems/man/en/chapter_5.html) which is used to run different tasks provided by the project

    ```sh
    curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | sudo bash -s -- --to /usr/bin
    ```

    Check out the [justfile website](https://just.systems/man/en/chapter_5.html) for more installation options.

### Customizing your own image

1. Fork the project

2. Edit the authorized ssh keys for each profile

    Edit the `root_authorized_keys` under each toml file in the `profiles/` directory, and add the public ssh keys whom you want to grant ssh access to for the device. Below shows one example.

    ```toml title="file: profiles/default.toml"
    #...

    [parameters.ssh]
    root_authorized_keys = """
    ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQDfhQGWWw73ponAokdNSRZ5cQc9/CIX1TLQgYlr+BtObKoO4UNFP1YSbgK03GjhjeUid+QPmV+UURqxQTqLQoYWqUFP2CYkILFccVPmTvx9HLwupI+6QQKWfMDx9Djfph9GzInymaA5fT7hKppqittFrC/l3lkKgKTX5ohEOGshIbRgtgOYIaW3ByTx3urnaBbYCIgOyOZzSIyS0dUkwsiLu3XjPspgmn3Fs/+vofT/yhBe1carW0UM3ivV0JFfJzrxbCl/F7I2qwfjZXsypjkwlpNupUMuo3xPMi8YvNvyEu4d+IEAqO1dCcdGcxlkiHxrdITIpVLt5mjJ2LauHE/H bootstrap
    """

    # ...
    ```

3. Commit the changes

4. You can now add any custom recipes to your project if you want to include additional packages in your image, or configure items.

    See the [Rugpi System Customization](https://oss.silitics.com/rugpi/docs/guide/system-customization) docs for details on how to do this or follow clues from some of the existing recipes included in the [tedge-rugpi-images](https://github.com/thin-edge/tedge-rugpi-image) project.

### Building an image

:::info
Currently building is only supported on a Linux environment. It is strongly encouraged to use the Github workflow to build the image.
:::

1. Build an image for your device (machine)

    ```sh tab={"label":"Pi\tZero"}
    just IMAGE_ARCH=armhf PROFILE=armhf VARIANT=pi01 build-all
    ```

    ```sh tab={"label":"Pi\t1"}
    just IMAGE_ARCH=armhf PROFILE=armhf VARIANT=pi01 build-all
    ```    

    ```sh tab={"label":"Pi\t2"}
    just IMAGE_ARCH=arm64 PROFILE=default VARIANT=pi023 build-all
    ```
    ```sh tab={"label":"Pi\t3"}
    just IMAGE_ARCH=arm64 PROFILE=default VARIANT=pi023 build-all
    ```
    ```sh tab={"label":"Pi\tZero2W"}
    just IMAGE_ARCH=arm64 PROFILE=default VARIANT=pi023 build-all
    ```

    ```sh tab={"label":"Pi\t4\t(With\tFirmware)"}
    just IMAGE_ARCH=arm64 PROFILE=default VARIANT=pi4 build-all
    ```

    ```sh tab={"label":"Pi\t4\t(Without\tFirmware)"}
    just IMAGE_ARCH=arm64 PROFILE=default VARIANT=pi45 build-all
    ```

    ```sh tab={"label":"Pi\t5"}
    just IMAGE_ARCH=arm64 PROFILE=default VARIANT=pi45 build-all
    ```

2. Inspect the built image

    ```sh
    ls -ltr build/*.img
    ```

3. Flash the base image using the instructions on the [Flashing an image](../../flashing-an-image) page


## Tips

This section contains general tips which can be helpful whilst either getting things setup, or what to do when you encounter an error.

### Building on MacOS Apple Silicon

Currently building an image is not supported on MacOS as it requires disks 

```
fallocate: fallocate failed: Operation not supported
```
