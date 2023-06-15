---
title: Supported Platform
tags: [Reference, Installation]
sidebar_position: 1
---

# thin-edge.io platform support

Common requirements for all systems are:
* minimum 16MB of RAM
* systemd (for production systems)
* mosquitto minimum version 1.6 (for security reasons we recommend the latest 1.x version)
* dpkg (if you want to use our prebuilt deb packages)

# Level 1
Level 1 supported platforms are officially supported and are actively tested in the CI/CD.
* ARMv7 Raspberry Pi OS 10
* ARMv8 Raspberry Pi OS 10
* AMD64 Ubuntu 20.04

# Level 2
Level 2 platforms are not officially supported and tested yet, but we know from our experiences that these systems used to work for some maintainers or users. If your os is not listed here, this does not mean it is not working, just give it a try. We are happy to hear about your experience in the Github discussions.
* Ubuntu 20.04 in WSL (only for development, not for running thin-edge.io due to missing systemd)
* AMD64 Debian 10
* ARMv6 Raspberry Pi OS 10 (needs to be built for this specific target, please refer to [Issue-161](https://github.com/thin-edge/thin-edge.io/issues/161))
* ARMv7 Raspberry Pi OS 11
* ARMv8 Raspberry Pi OS 11
