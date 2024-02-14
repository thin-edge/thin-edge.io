## Package scripts

Package scripts (aka maintainer scripts) are the scripts which are run when the package is installed and removed. The maintainer scripts differ slightly between the different linux package types (e.g. deb, rpm and apk), so in order to minimize maintenance effort, an generator script (`generate.py`) is used to convert the common parts of the maintainer scripts to the package specific scripts.

The general workflow for editing the maintainer scripts is highlighted in the diagram below.

```mermaid
graph LR
    edit[edit] --> generate[generate] --> review --> commit --> package[build package]
```

### How does the maintainer script generation work?

The maintainer script generation is controlled by the following two files:

* `configuration/package_scripts/generate.py`
* `configuration/package_scripts/packages.json`

The `generate.py` script takes the template input variables defined in `packages.json` and generates the corresponding maintainer scripts, one per package type. The generator script looks for any manually created maintainer scripts (e.g. preinst, postinst, prerm and postrm) and expands any referenced template variables.

The `packages.json` file includes which packages to build and additional information about which services are packaged inside the script and how to handle the services during installation, upgrade and removal. This logic is referred to as the *systemd snippets*.

The *systemd snippets* are a collection of snippets which are added to the maintainer scripts based on the settings defined in the `packages.json`. For instance, the `packages.json` file controls whether a service should be automatically started and enabled, and if the service should be restarted during an upgrade. The different combination of settings controls which snippets are selected for injection into the input maintainer scripts.

The concept of *systemd snippets* is not new, different linux packaging formats have their own variant of this, however we drew our inspiration from the Debian based [debhelper](https://man7.org/linux/man-pages/man7/debhelper.7.html), and the template generation logic from [cargo-deb](https://github.com/kornelski/cargo-deb).

The *systemd snippet* are added into the manually created maintainer scripts by adding the following magic comment:

```sh
#LINUXHELPER#
```

When the `generate.py` script is run, the `#LINUXHELPER#` text will be replaced with the appropriate systemd snippet for the package manager type.

The snippets are stored under the `configuration/package_scripts/templates/` directory where each package type can define its own snippets.

```text
$ ls -l configuration/package_scripts/templates/

total 0
drwxr-xr-x@ 11 developer  staff  352 Aug  8 17:26 apk
drwxr-xr-x@ 11 developer  staff  352 Aug  8 17:26 deb
drwxr-xr-x@ 11 developer  staff  352 Aug  8 17:26 rpm
```

The list of deb (debian) snippets are shown below. The contents of the snippets was sourced from [cargo-deb](https://github.com/kornelski/cargo-deb/tree/main/autoscripts) which inturn sourced it from the [debian autoscripts](https://github.com/Debian/debhelper/tree/master/autoscripts).

```text
$ ls -l configuration/package_scripts/templates/deb

total 72
-rw-r--r--@ 1 developer  staff  661 Aug  8 17:26 postinst-systemd-dont-enable
-rw-r--r--@ 1 developer  staff  746 Aug  8 17:26 postinst-systemd-enable
-rw-r--r--@ 1 developer  staff  373 Aug  8 17:26 postinst-systemd-restart
-rw-r--r--@ 1 developer  staff  321 Aug  8 17:26 postinst-systemd-restartnostart
-rw-r--r--@ 1 developer  staff  281 Aug  8 17:26 postinst-systemd-start
-rw-r--r--@ 1 developer  staff  339 Aug  8 17:26 postrm-systemd
-rw-r--r--@ 1 developer  staff   91 Aug  8 17:26 postrm-systemd-reload-only
-rw-r--r--@ 1 developer  staff   94 Aug  8 17:26 prerm-systemd
-rw-r--r--@ 1 developer  staff  115 Aug  8 17:26 prerm-systemd-restart
```

### Editing a maintainer script

Generally speaking, you should only add logic to a maintainer script which is generic for all the different package managers. You should not put any service specific logic in manually as this should be automatically provided by the *systemd snippets*.

Below shows the general workflow to editing an existing maintainer script for the `tedge` package.

1. Modify an existing package script under the `configuration/package_scripts` folder

    For example, you can modify the post installation script for the `tedge` component by editing the following file:

    ```text
    configuration/package_scripts/tedge/postinst
    ```

    **Note**

    Never modify the files in the `configuration/package_scripts/_generated/` directory. These files will be overwritten automatically when the generation script is triggered

2. Generate the output maintainer scripts

    ```sh
    just generate-linux-package-scripts
    ```

3. Review the output in the `configuration/package_scripts/_generated/` folder using the git diff

4. Commit both the original changes and the auto generated files

5. Now you can build the linux packages by running the `release` task

    ```sh
    just release
    ```

6. View the packages under the `packages` folder under the relevant target

    For example, if my machine was running aarch64, then it would automatically use the `aarch64-unknown-linux-musl` target, and the packages would be available under:

    ```sh
    ls -l  target/aarch64-unknown-linux-musl/packages
    ```

    ```text
    -rw-r--r--@ 1 developer  staff   2506444 Aug  8 11:47 c8y-firmware-plugin-0.11.1~340+g1441e71d.aarch64.rpm
    -rw-r--r--@ 1 developer  staff   2490272 Aug  8 11:47 c8y-firmware-plugin_0.11.1~340+g1441e71d_aarch64.apk
    -rw-r--r--@ 1 developer  staff   2018558 Aug  8 11:47 c8y-firmware-plugin_0.11.1~340+g1441e71d_arm64.deb
    -rw-r--r--@ 1 developer  staff   1735095 Aug  8 11:47 c8y-remote-access-plugin-0.11.1~340+g1441e71d.aarch64.rpm
    -rw-r--r--@ 1 developer  staff   1737980 Aug  8 11:47 c8y-remote-access-plugin_0.11.1~340+g1441e71d_aarch64.apk
    -rw-r--r--@ 1 developer  staff   1413456 Aug  8 11:47 c8y-remote-access-plugin_0.11.1~340+g1441e71d_arm64.deb
    -rw-r--r--@ 1 developer  staff   2463953 Aug  8 11:47 tedge-0.11.1~340+g1441e71d.aarch64.rpm
    -rw-r--r--@ 1 developer  staff   2862011 Aug  8 11:47 tedge-agent-0.11.1~340+g1441e71d.aarch64.rpm
    -rw-r--r--@ 1 developer  staff   2865115 Aug  8 11:47 tedge-agent_0.11.1~340+g1441e71d_aarch64.apk
    -rw-r--r--@ 1 developer  staff   2287988 Aug  8 11:47 tedge-agent_0.11.1~340+g1441e71d_arm64.deb
    -rw-r--r--@ 1 developer  staff   1163398 Aug  8 11:47 tedge-apt-plugin-0.11.1~340+g1441e71d.aarch64.rpm
    -rw-r--r--@ 1 developer  staff   1162657 Aug  8 11:47 tedge-apt-plugin_0.11.1~340+g1441e71d_aarch64.apk
    -rw-r--r--@ 1 developer  staff    912366 Aug  8 11:47 tedge-apt-plugin_0.11.1~340+g1441e71d_arm64.deb
    -rw-r--r--@ 1 developer  staff   3016043 Aug  8 11:47 tedge-mapper-0.11.1~340+g1441e71d.aarch64.rpm
    -rw-r--r--@ 1 developer  staff   2975789 Aug  8 11:47 tedge-mapper_0.11.1~340+g1441e71d_aarch64.apk
    -rw-r--r--@ 1 developer  staff   2394964 Aug  8 11:47 tedge-mapper_0.11.1~340+g1441e71d_arm64.deb
    -rw-r--r--@ 1 developer  staff   1674451 Aug  8 11:47 tedge-watchdog-0.11.1~340+g1441e71d.aarch64.rpm
    -rw-r--r--@ 1 developer  staff   1671834 Aug  8 11:47 tedge-watchdog_0.11.1~340+g1441e71d_aarch64.apk
    -rw-r--r--@ 1 developer  staff   1373832 Aug  8 11:47 tedge-watchdog_0.11.1~340+g1441e71d_arm64.deb
    -rw-r--r--@ 1 developer  staff  19135164 Aug  8 11:47 tedge_0.11.1~340+g1441e71d_aarch64-unknown-linux-musl.tar.gz
    -rw-r--r--@ 1 developer  staff   2450199 Aug  8 11:47 tedge_0.11.1~340+g1441e71d_aarch64.apk
    -rw-r--r--@ 1 developer  staff   1976064 Aug  8 11:47 tedge_0.11.1~340+g1441e71d_arm64.deb
    ```
