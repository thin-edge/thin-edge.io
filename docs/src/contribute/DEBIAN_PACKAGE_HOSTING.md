---
title: Debian Package Hosting
tags: [Contribute, Packaging]
sidebar_position: 3
---

# Debian Package Hosting

In additional to the thin-edge.io install script, the packages are also publicly hosted APT repository.

## Official releases

The following APT repositories contain the official releases of thin-edge.io. The packages will have a nice `x.y.z` version number, and all of the packages go through our full suite of automated and manual testing.

The packages are the same ones which are uploaded to the [GitHub Releases](https://github.com/thin-edge/thin-edge.io/releases) page.

### tedge-release (default)

This is the default repository that most users should be using.

**Setup script**
```sh
curl -1sLf \
  'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.deb.sh' \
  | sudo -E bash
```

**Supported Architectures**
* `amd64`
* `arm64`
* `armhf` (armv7)

### tedge-release-armv6

If you are using a Raspberry Pi with a `armv6l` CPU Architecture, then you will need to use this repository.

```sh
curl -1sLf \
  'https://dl.cloudsmith.io/public/thinedge/tedge-release-armv6/setup.deb.sh' \
  | sudo -E bash
```

**Supported Architectures**
* `armhf` (armv6)

## Pre releases

The latest built packages from the `main` branch of the project. The packages go through the same automated testing process as the official releases, however they are not tagged in git, so the version numbers will look like `0.8.1-171-ga72e5432` (see the [Version syntax](./DEBIAN_PACKAGE_HOSTING.md#version-syntax) for description about the version).

These repositories allow you to test new features as they get merged to `main`, rather than waiting for the official release. However it is still advised to only use these repositories for development and testing purposes as the official versions go through additional testing.

### tedge-main

```sh
curl -1sLf \
  'https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.deb.sh' \
  | sudo -E bash
```

**Supported Architectures**
* `amd64`
* `arm64`
* `armhf` (armv7)

### tedge-main-armv6

If you are using a Raspberry Pi with a `armv6l` CPU Architecture, then you will need to use this repository.

```sh
curl -1sLf \
  'https://dl.cloudsmith.io/public/thinedge/tedge-main-armv6/setup.deb.sh' \
  | sudo -E bash
```

**Supported Architectures**
* `armhf` (armv6)



## Installing and upgrading

1. Assuming you have already configured the appropriate APT repository, you will need to make sure it is up to date.

    ```sh
    sudo apt-get update
    ```

2. Install/update each of the packages

    ```sh
    sudo apt-get install -y \
        tedge \
        tedge-mapper \
        tedge-agent \
        tedge-watchdog \
        c8y-configuration-plugin \
        tedge-apt-plugin \
        c8y-firmware-plugin \
        c8y-remote-access-plugin \
        c8y-log-plugin
    ```

    The latest version will be automatically selected by `apt-get`.

## Removing the APT repositories

All of the `thin-edge.io` apt repositories can be removed by the following command.

```sh
sudo rm -f /etc/apt/sources.list.d/thinedge-tedge-*.list
sudo apt-get clean
sudo rm -rf /var/lib/apt/lists/*
sudo apt-get update
```

## Version syntax

The version is automatically generated from the source code management tool, git. The version is based on the commit used to build the packages and its distance from the last tag (e.g. the last official released version).

```sh
{base_version}-{distance}g{git_sha}

# Example
0.8.1-171-ga72e5432
```

|Part|Description|
|----|-----------|
|`base_version`|Last official release|
|`distance`|Number of commits on the `main` branch since last official release|
|`git_sha`|Git commit sha which the package was built from. This makes it easier to trace the version back to the exact commit|

# How is this made possible?

Package repository hosting is graciously provided by [Cloudsmith](https://cloudsmith.com).
Cloudsmith is the only fully hosted, cloud-native, universal package management solution, that
enables your organization to create, store and share packages in any format, to any place, with total
confidence.

[![Hosted By: Cloudsmith](https://img.shields.io/badge/OSS%20hosting%20by-cloudsmith-blue?logo=cloudsmith&style=flat-square)](https://cloudsmith.com)
