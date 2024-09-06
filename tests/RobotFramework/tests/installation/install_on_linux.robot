*** Settings ***
Resource        ../../resources/common.resource
Library         Collections
Library         ThinEdgeIO

Test Tags       theme:installation    test:on_demand


*** Variables ***
# Debian
${APT_SETUP}        apt-get update \
...                 && apt-get install -y sudo curl mosquitto \
...                 && curl -1sLf https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.deb.sh | sudo -E bash
${APT_INSTALL}      apt-get install -y tedge-full

# CentOS/RHEL
${DNF_SETUP}        dnf install -y epel-release \
...                 && dnf install -y sudo mosquitto \
...                 && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.rpm.sh" | sudo -E bash
${DNF_INSTALL}      dnf install -y tedge-full

${SUSE_SETUP}
...                 zypper install -y sudo curl mosquitto \
...                 && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.rpm.sh" | sudo -E version\=any-version codename\="" bash
${SUSE_INSTALL}     zypper install -y tedge-full

# DNF where the epel-release repo is not required (e.g. Fedora)
${DNF2_SETUP}       dnf install -y sudo mosquitto \
...                 && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.rpm.sh" | sudo -E bash
${DNF2_INSTALL}     dnf install -y tedge-full

# Microdnf
${MDNF_SETUP}       microdnf install -y epel-release \
...                 && microdnf install -y sudo tar mosquitto \
...                 && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.rpm.sh" | sudo -E bash
${MDNF_INSTALL}     microdnf install -y tedge-full

# Alpine linux
${APK_SETUP}        apk add --no-cache sudo curl bash mosquitto \
...                 && curl -1sLf "https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.alpine.sh" | sudo -E bash
${APK_INSTALL}      apk add --no-cache tedge-full


*** Test Cases ***
Install on CentOS/RHEL based images
    [Template]    Install using dnf
    rockylinux:9
    almalinux:8

Install on CentOS/RHEL (microdnf) based images
    [Template]    Install using microdnf
    rockylinux:9-minimal

Install on Fedora based images
    [Template]    Install using fedora dnf
    fedora:38
    fedora:37

Install on OpenSUSE based images
    [Template]    Install using zypper
    opensuse/leap:15
    opensuse/tumbleweed:latest

Install on Debian based images
    [Template]    Install using apt
    debian:10-slim
    debian:11-slim
    ubuntu:20.04
    ubuntu:22.04
    ubuntu:23.04

Install on Alpine based images
    [Template]    Install using apk
    alpine:3.18
    alpine:3.17
    alpine:3.16

Install on any linux distribution
    [Template]    Install using script
    alpine:3.18
    alpine:3.17
    alpine:3.16
    debian:12-slim
    busybox
    ubuntu:23.04
    opensuse/leap:15
    opensuse/tumbleweed:latest
    rockylinux:9-minimal
    rockylinux:8-minimal
    almalinux:9-minimal
    almalinux:8-minimal
    debian:12-slim    install_args=--package-manager tarball


*** Keywords ***
Install using dnf
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=    Setup    skip_bootstrap=${True}
    Execute Command    ${DNF_SETUP}
    Execute Command    ${DNF_INSTALL}

Install using microdnf
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=    Setup    skip_bootstrap=${True}
    Execute Command    ${MDNF_SETUP}
    Execute Command    ${MDNF_INSTALL}

Install using fedora dnf
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=    Setup    skip_bootstrap=${True}
    Execute Command    ${DNF2_SETUP}
    Execute Command    ${DNF2_INSTALL}

Install using apt
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=    Setup    skip_bootstrap=${True}
    Execute Command    ${APT_SETUP}
    Execute Command    ${APT_INSTALL}

Install using apk
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=    Setup    skip_bootstrap=${True}
    Execute Command    ${APK_SETUP}    shell=${True}    sudo=${False}
    Execute Command    ${APK_INSTALL}

Install using zypper
    [Arguments]    ${IMAGE}
    Set To Dictionary    ${DOCKER_CONFIG}    image=${IMAGE}
    ${DEVICE_ID}=    Setup    skip_bootstrap=${True}
    Execute Command    ${SUSE_SETUP}    shell=${True}    sudo=${False}
    Execute Command    ${SUSE_INSTALL}

Install using script
    [Arguments]    ${image}    ${pre_install}=    ${install_args}=
    Set To Dictionary    ${DOCKER_CONFIG}    image=${image}
    ${DEVICE_ID}=    Setup    skip_bootstrap=${True}

    IF    "${pre_install}" != ""
        Execute Command    ${pre_install}    sudo=${False}    timeout=2
    END

    Transfer To Device    ${CURDIR}/../../../../install.sh    /setup/
    Execute Command    chmod +x /setup/install.sh && /setup/install.sh ${install_args}    sudo=${False}    timeout=2
    Validate Installation

Validate Installation
    Execute Command    timeout 2 tedge-agent || exit 0    timeout=2
