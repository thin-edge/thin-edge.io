---
title: Setting up Docker
tags: [Contribute]
sidebar_position: 10
---

# Setting up Docker (OSS - Variant)

This guide is to assist with the installation of docker and some of the docker cli tools. It is not meant to replace other online articles but more of a cheat sheet to getting your setup up and running as fast as possible. If you encounter issues with the setup, please search online for a fix.

## Installation

### MacOS (amd64/arm64)

There are different ways to install the docker engine on MacOS, however this guide will focus on using [colima](https://github.com/abiosoft/colima) which uses `limactl` under the hood to create a virtual machine and supporting functionality such as automatic port forwarding.

1. Install homebrew. You can also install homebrew without `sudo` rights using this [guide](https://docs.brew.sh/Installation#untar-anywhere-unsupported)

    :::tip
    Make sure you read the homebrew instructions which are printed out on the console as it will guide you to the additional setup steps.
    :::

2. Install [colima](https://github.com/abiosoft/colima) and the docker cli

    ```
    brew install colima
    ```

    :::info
    The installation will take a while as it will have to compile various dependencies.
    :::

3. Install the docker cli tools

    ```
    brew install docker docker-compose docker-credential-helper
    ```

    Then configure `docker-compose` as a docker plugin so that you can use `docker compose` instead of the legacy `docker-compose` script.

    ```
    mkdir -p ~/.docker/cli-plugins
    ln -sfn $(brew --prefix)/opt/docker-compose/bin/docker-compose ~/.docker/cli-plugins/docker-compose
    ```

4. Configure and start `colima`

    ```
    colima start --cpu 4 --memory 8
    ```

    :::info
    * The extra arguments are only needed the first time you start it. You can change the settings to based on what you are need/have.
    * You can adjust the number of CPUs and amount of RAM used by colima to suite your machine
    * You will have to start `colima` each time your machine is restarted (as it is not configured to autostart, though there are ways on MacOS to do this)
    :::

5. Check that everything is working by starting the `hello-world` container

    ```
    docker run -it --rm hello-world
    ```


**References**

* https://smallsharpsoftwaretools.com/tutorials/use-colima-to-run-docker-containers-on-macos/


### Windows (WSL2)

If you are using Windows, it is recommended to use WSL2 to run your a Ubuntu distribution where `docker-ce` is installed within it. Please do not bother with Docker Desktop or any other "Desktop" related product (e.g. `Rancher Desktop`). It will save you a lot of hassle by using the native `docker-ce` version inside the WSL2 distribution itself.

Once you have a Ubuntu distribution running under WSL2, follow the instructions under the [Linux installation](./DEV_CONTAINER.md#Linux) section.

### Linux

Checkout the [online documentation](https://docs.docker.com/engine/install/) how to install the docker engine (`docker-ce`) on your linux distribution. Don't forget to run the [Linux postinstall instructions](https://docs.docker.com/engine/install/linux-postinstall/) to enable docker to be controlled by non-root linux user.

The following steps should be executed:

* Install [docker-ce](https://docs.docker.com/engine/install/)
* Install [docker compose plugin](https://docs.docker.com/compose/install/)
* Run docker-ce [post installation instructions](https://docs.docker.com/engine/install/linux-postinstall/)

After the install verify that everything is working correctly by running the following commands

1. Check docker

    ```sh
    docker run -it --rm hello-world
    ```

2. Verify that the `docker compose` plugin was configured correctly

    ```sh
    docker compose --help
    ```

## Troubleshooting

Some typical problems can be found here. Remember, if you don't find your error then please search online using your preferred search engine.

### Invalid

Check the docker config file `~/.docker/config.json`. The credential manager setting (`credsStore`) should be set to `osxkeychain`.

**Example: ~/.docker/config.json**

```json
{
	"auths": {},
	"credsStore": "osxkeychain",
	"currentContext": "colima"
}
```
