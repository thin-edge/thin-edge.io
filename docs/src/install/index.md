---
title: Install
tags: [Installation]
sidebar_position: 1
---

:::tip Announcement
%%te%% ❤️ Linux so we now support installing %%te%% on any Linux distribution!

Systemd is still the default init system (aka. service manager), however if you don't have Systemd, then it won't be used. You are then free to configure your own service manager to run %%te%% how you want, or use one of the [community supported packages](#optional-linux-distributions-without-systemd).
:::

## Install/update

The easiest way to get started with %%te%% is to use the installation script which will auto detect the installation method appropriate for your Linux distribution. The script will configure the package manager and install %%te%% and its dependencies (e.g. mosquitto). If your distribution does not have one of the supported package managers, then the tarball will be used to install %%te%%.

To install or update to the latest version, run the following command:

```sh tab={"label":"curl"}
curl -fsSL https://thin-edge.io/install.sh | sh -s
```

```sh tab={"label":"wget"}
wget -O - https://thin-edge.io/install.sh | sh -s
```

If the device is in a network behind a HTTP Proxy, then you will need to set the `https_proxy` environment variable before installing %%te%%.

```sh
export https_proxy="http://<username>:<password>@<ip>:<port>"

# Without authentication
export https_proxy="http://127.0.0.1:8080"

# With authentication
export https_proxy="http://user:password@127.0.0.1:8080"
```

### Update using a package manager

%%te%% and its components can be updated by running the install.sh script again, or using the Linux package manager on your distribution.

```sh tab={"label":"Debian/Ubuntu"}
sudo apt-get update
sudo apt-get install tedge-full
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
sudo dnf install --best tedge-full --refresh
```

```sh tab={"label":"Alpine"}
sudo apk update
sudo apk add --no-cache tedge-full
```

:::info
If you have any trouble updating via the package manager, then run the install.sh script again. The install script will install or update the script as well as configure the appropriate package manager for your Linux distribution.
:::

### Optional: Linux distributions without Systemd

%%te%% uses Systemd by default to run all of its components as background services. If your Linux distribution does not have Systemd installed, then you will also have to run one more additional step.

Run the script below to automatically detect and install the relevant service definitions for the init system provided by your Linux distribution.

```sh tab={"label":"curl"}
curl -fsSL https://thin-edge.io/install-services.sh | sh -s
```

```sh tab={"label":"wget"}
wget -O - https://thin-edge.io/install-services.sh | sh -s
```

Check out the [init systems](../operate/installation/init-systems.md) for more information on the service definitions.

:::info
If the script detects that Systemd is installed, then it will not install anything as %%te%% comes with Systemd service definitions.
:::

## Supported Linux Package Managers

The following Linux Package Managers are supported out-of-the-box. For all other Linux distributions, the tarball (.tar.gz binary) can be used to add the %%te%% executables/binaries.

<div>
    <div class="row">
        <div class="column logo">
            <em><img width="80" height="80" src="https://assets.cloudsmith.media/package/images/backends/deb/large.30f93502b7b5.png" alt="Debian logo" /></em>
        </div>
        <div class="column logo">
            <em><img width="80" height="80" src="https://assets.cloudsmith.media/package/images/backends/rpm/large.f677f5642875.png" alt="RedHat logo" /></em>
        </div>
        <div class="column logo">
            <em><img width="80" height="80" src="https://assets.cloudsmith.media/package/images/backends/alpine/large.974a497e9765.png" alt="Alpine logo" /></em>
        </div>
    </div>
</div>

|Package Manager|Format|Distributions|
|---------------|------|-------------|
|apt|deb|Debian, Ubuntu and other debian-based operating systems|
|yum/dnf/microdnf|rpm|RHEL, RockyLinux, AlmaLinux, Fedora|
|zypper|rpm|openSUSE|
|apk|apk|Alpine Linux|
|-|tarball (*.tar.gz)|All other Linux distributions, e.g. Yocto|


## Alternative installation methods

In cases were you would not like to run the automatic install script, you can choose one to run the steps manually. This allows you more control over the process which can be useful if you are experiencing problems with the auto detection used in the install script.

### Manual repository setup and installation

The software repositories used by the package managers can be configured using the setup scripts. These scripts are normally executed by the *install.sh* script in the installation section, however they can also be manually executed if you want more fine-grain control over the process.

:::tip
If you are having problems setting any of the repositories, check out the [Cloudsmith](https://cloudsmith.io/~thinedge/repos/tedge-release/setup/#formats-deb) website where they have **Set Me Up** instructions in additional formats, e.g. manual configuration rather than via the `setup.*.sh` script.
:::

**Pre-requisites**

The instructions require you to have the following tools installed.

* curl
* bash

#### Setup

**Running with sudo**

You will need to have `sudo` also installed if you want to run these instructions.

```sh tab={"label":"Debian/Ubuntu"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.deb.sh' | sudo bash
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.rpm.sh' | sudo bash
```

```sh tab={"label":"Alpine"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.alpine.sh' | sudo bash
```

**Running as root**

These commands must be run as the root user.

```sh tab={"label":"Debian/Ubuntu"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.deb.sh' | bash
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.rpm.sh' | bash
```

```sh tab={"label":"Alpine"}
curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.alpine.sh' | bash
```


#### Installing and updating using a package manager

Once you have the repository setup, you can install the **tedge-full** virtual package which will automatically pull in all of the %%te%% packages. This makes it easier to install and update in the future, as you only have to type in one package name, `tedge-full`.

```sh tab={"label":"Debian/Ubuntu"}
sudo apt-get update
sudo apt-get install tedge-full
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
sudo dnf install --best tedge-full --refresh
```

```sh tab={"label":"Alpine"}
sudo apk update
sudo apk add --no-cache tedge-full
```

### Install via tarball

You can force the install.sh script to install via the tarball instead of via a package manager. The install script will also take care of the required post installation steps.

To install the %%te%% via the tarball run the following command:


```sh tab={"label":"curl"}
curl -fsSL https://thin-edge.io/install.sh | sh -s -- --package-manager tarball
```

```sh tab={"label":"wget"}
wget -O - https://thin-edge.io/install.sh | sh -s -- --package-manager tarball
```

## Community plugins

In addition to the core %%te%% packages,
there are several community plugins developed and maintained by the %%te%% open source community.
These plugins provide additional functionality and integrations for %%te%%,
enabling users to extend the capabilities of their IoT solutions.
Some of the popular community plugins are listed [here](../operate/plugins/index.md).

:::note
The community plugins maintained by the %%te%% team are hosted in a separate [community repository on Cloudsmith](https://cloudsmith.io/~thinedge/repos/community/packages).
Instructions to setup this repo are documented [here](https://cloudsmith.io/~thinedge/repos/community/setup).
:::

## Package repository hosting

[_![Hosted By: Cloudsmith](https://img.shields.io/badge/OSS%20hosting%20by-cloudsmith-blue?logo=cloudsmith&style=for-the-badge)_](https://cloudsmith.com)

Package repository hosting is graciously provided by  [Cloudsmith](https://cloudsmith.com).
Cloudsmith is the only fully hosted, cloud-native, universal package management solution, that
enables your organization to create, store and share packages in any format, to any place, with total
confidence.

The packages can be viewed directly from the [Cloudsmith.io](https://cloudsmith.io/~thinedge/repos/) website.

<table>
<tr>
    <th>Linux</th>
    <th>Repository</th>
</tr>
<tr>
    <td>
        <em><img width="24" height="24" src="https://assets.cloudsmith.media/package/images/backends/deb/small.bedd6f749317.png" alt="Debian logo" /></em>
    </td>
    <td>
        <a href="https://cloudsmith.io/~thinedge/repos/tedge-release/packages/detail/deb/tedge-full/latest/a=all;d=any-distro%252Fany-version;t=binary/"><em><img src="https://api-prd.cloudsmith.io/v1/badges/version/thinedge/tedge-release/deb/tedge-full/latest/a=all;d=any-distro%252Fany-version;t=binary/?render=true&show_latest=true" alt="Latest version of 'tedge-full' @ Cloudsmith" /></em></a>
    </td>
</tr>
<tr>
    <td>
        <em><img width="24" height="24" src="https://assets.cloudsmith.media/package/images/backends/rpm/small.89bd26d9d17b.png" alt="RedHat logo" /></em>
    </td>
    <td>
        <a href="https://cloudsmith.io/~thinedge/repos/tedge-release/packages/detail/rpm/tedge-full/latest/a=noarch;d=any-distro%252Fany-version;t=binary/"><em><img src="https://api-prd.cloudsmith.io/v1/badges/version/thinedge/tedge-release/rpm/tedge-full/latest/a=noarch;d=any-distro%252Fany-version;t=binary/?render=true&show_latest=true" alt="Latest version of 'tedge-full' @ Cloudsmith" /></em></a>
    </td>
</tr>
<tr>
    <td>
        <em><img width="24" height="24" src="https://assets.cloudsmith.media/package/images/backends/alpine/small.dff9b535ea47.png" alt="Alpine logo" /></em>
    </td>
    <td>
        <a href="https://cloudsmith.io/~thinedge/repos/tedge-release/packages/detail/alpine/tedge-full/latest/a=noarch;d=alpine%252Fany-version/"><em><img src="https://api-prd.cloudsmith.io/v1/badges/version/thinedge/tedge-release/alpine/tedge-full/latest/a=noarch;d=alpine%252Fany-version/?render=true&show_latest=true" alt="Latest version of 'tedge-full' @ Cloudsmith" /></em></a>
    </td>
</tr>
</table>

## Shell completions

:::note
Shell-completions are supported with `tedge` >= 1.5.0
:::

To make using the Command Line Interface (CLI) easier, `tedge` provides shell completion support (also known as tab-completions). Shell completions allow users to discover commands and configuration directly on the command line without having to check the online documentation.
For all `tedge` commands, you can activate the tab completion by pressing `<TAB>` twice in quick succession. You will be presented with the available options based on the given the command line's current input.

Before you can use shell completions, you need to enable them first. If you've configured tab completion for other packages already, then you may be able to skip these steps. The following sections include instructions on how to load the completions which are included in the %%te%% Linux packages, or how to load the completions at runtime by running the `tedge completions <SHELL_TYPE>` command.

Each type of shell (e.g. bash, zsh, fish) offers slightly different shell completion user experiences, where zsh and fish are generally more verbose than bash, so to give you idea what to expect, the following examples show some examples for various shells (where `<TAB>` represents pressing the "TAB" button on the keyboard).

```sh title="bash completion to set the log level"
$ tedge connect c8y --log-level <TAB><TAB>
trace  debug  info   warn   error
```

```sh title="zsh completion to set a specific configuration item"
$ tedge config set mqtt.client.<TAB><TAB>
mqtt.client.auth.ca_dir     --  Path to the directory containing the CA certificates used by MQTT
mqtt.client.auth.ca_file    --  Path to the CA certificate used by MQTT clients to use when authenticating the MQTT broker
mqtt.client.auth.cert_file  --  Path to the client certificate
mqtt.client.auth.key_file   --  Path to the client private key
mqtt.client.host            --  The host that the thin-edge MQTT client should connect to
mqtt.client.port            --  The port that the thin-edge MQTT client should connect to
```

```sh title="fish completion to find a subcommand"
$ tedge con<TAB><TAB>
config  (Configure Thin Edge)  connect  (Connect to cloud provider)
```

### Loading completions from Linux packages {#load-packaged-completions}

The %%te%% Linux packages (starting from 1.5.0) will install the shell completion scripts during the package's post installation phase. However, post installation
will only add the completions scripts if it detects the presence of the associated shells (e.g. `bash`, `zsh` and/or `fish`).

Using the completions included in the Linux packages offers a better out of the box experience as some operating systems or shells don't require any setup.
If your operating system does not enable completions for you, then follow the instructions for your preferred shell to enable the shell completions.


### bash

For bash completions, you will need to install the **bash-completion** package for your operating system. If you can't find instructions for your operating system, then please consult online resources.

```sh tab={"label":"Debian/Ubuntu"}
sudo apt-get update
sudo apt-get install bash-completion
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
sudo dnf install --best bash-completion --refresh
```

```sh tab={"label":"Alpine"}
apk add bash-completion
```

After the **bash-completion** package is installed, then you will need to edit your shell profile by adding the following snippet, so that when bash shell starts, it will know that it should also load the available tab completions.

```sh title="~/.bashrc"
[ -f /etc/bash_completion ] && source /etc/bash_completion
```

Reload your shell profile after making any changes to it. You can reload your shell profile by running:

```sh
source ~/.bashrc
```

:::note
If the **bash-completion** package was not installed at the time %%te%% was installed then you can still activate completions as runtime by following [these instructions](#load-completions-at-runtime).
:::

### zsh

zsh does not require any package dependencies, however you will need to add the following snippet to your zsh profile:

```sh title="~/.zshrc"
# refresh completions
autoload -U compinit; compinit

# Optional: Improve styling of menu complete
zstyle ':completion:*' menu select
# bind shift+tab to reverse menu complete
zmodload zsh/complist
bindkey -M menuselect '^[[Z' reverse-menu-complete
```

Reload your shell profile after making any changes to it. You can reload your shell profile by running:

```sh
source ~/.zshrc
```

### fish

fish completions should just work out of the box without any additional dependencies or editing your shell profile.

### Loading completions at runtime {#load-completions-at-runtime}

The shell completions can also be loaded at runtime in your current shell by running one of the following commands associated with your shell.
The completions will only be loaded into your current shell's session, so you will need to run the command every time you open a new shell.

```sh tab={"label":"bash"}
source <(tedge completions bash)
```

```sh tab={"label":"zsh"}
source <(tedge completions zsh)
```

```sh tab={"label":"fish"}
tedge completions fish | source
```

You can add the above line to your shell's profile manually to save you having to run the command each time.
