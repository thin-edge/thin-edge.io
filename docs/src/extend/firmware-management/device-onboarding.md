---
title: Onboarding a Device
tags: [Extend, Build]
sidebar_position: 5
description: Connecting devices to the cloud for the first time
---

Onboarding a new devices involves configuring %%te%% to connect to a cloud for the first time. This generally involves the following steps:

1. Configure the cloud url to connect to
2. Generate the device certificate (if it does not already exist)
3. Upload the device certificate to the cloud
4. Connect the associated tedge-mapper

## Onboarding a device to Cumulocity

The onboarding described in this section involved onboarding a device by executing remote commands on the device via SSH.

It is a two step process which is described as follows:

1. Detect the device (by using DNS Service Discovery)
2. Onboard it via ssh commands


### Pre-requisites

On your machine you will need on of the following:

1. One of the following CLI tools to scan for devices

    * avahi-browse
    * dns-sd

    :::tip MacOS Users
    **dns-sd** is installed by default.
    :::

2. A valid ssh key which matches the authorized key loaded in the built image

    The ssh key on your local machine should be loaded. You can check if your key is loaded into the `ssh-agent` on your host by running:

    ```sh
    ssh-add -l
    ```

    ```sh title="Output"
    2048 SHA256:abcdef/E1bA+DbVEsw3RoqKEMK9kU /home/powersuser/.ssh/id_rsa (RSA)
    ```

    :::info
    If you are using an image built using **Rugpi**, the authorized ssh key/s should have been included in the image. If you can't ssh into your device please review the [build instructions](../building-image/rugpi/tutorial/#clone-project) again to make sure the correct ssh key/s was added during build time.
    :::

3. [go-c8y-cli](https://goc8ycli.netlify.app/docs/installation/shell-installation/)

4. go-c8y-cli extension [c8y-tedge](https://github.com/thin-edge/c8y-tedge)


#### Setting up go-c8y-cli

1. Install [go-c8y-cli](https://goc8ycli.netlify.app/docs/installation/shell-installation/) as per the instructions

2. Install the [](https://github.com/thin-edge/c8y-tedge)

    ```sh
    c8y extension install thin-edge/c8y-tedge
    ```

3. Create a Cumulocity session file (if one does not already for the Cumulocity instance you wish to connect to)

    ```sh
    c8y sessions create
    ```

### Procedure

1. Insert the SD Card and power on the device, making sure the ethernet cable is also connected

2. Activate your go-c8y-cli session where you would like to onboard your device to

    ```sh
    set-session
    ```

3. Scan for the device using DNS Service Discovery (DNS-SD)

    ```sh
    c8y tedge scan
    ```

    ```sh title="Output"
    rpi1-b827eb21100e
    rpi2-b827ebed6e5a
    rpi3-b827ebe1f7d6
    rpi4-dcb630486720
    rpi5-d83add9a145a
    rpizero-b827ebdddb46
    rpizero2-d83add030bfd
    ```

    :::info
    The device may take a few minutes to start and run the initialization tasks, so be patient.

    If the device is still not connected after 10 minutes, then double check that the ethernet is connected correctly, and that you are in a network which assigns an automatic IP address via DHCP.
    :::

4. Bootstrap the image using the detected device in the previous scan

    ```
    c8y tedge bootstrap root@rpi5-d83add9a145a.local
    ```

5. If you have Wi-Fi on your device, then you can also connect to it so that you can remove the ethernet cable

    The `ncmli` command can be issued via ssh to configure and connect the device to a given Wi-Fi network:

    ```sh
    ssh root@mydevice.local nmcli device wifi connect "<AP name>" password "<password>"
    ```

    Where `<AP name>` is the Access Point Name (e.g. SSID) of the network, and the `<password>` is the password required to connect to the access point.
