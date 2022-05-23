# How to manage configuration files with Cumulocity

With `thin-edge.io`, you can manage config files on a device by using the [Cumulocity configuration management feature](https://cumulocity.com/guides/users-guide/device-management/#managing-configurations) as a part of Device Management.

If you are new to the Cumulocity **Configuration Management** feature,
we recommend you to read [the Cumulocity user guide](https://cumulocity.com/guides/users-guide/device-management/#managing-configurations) along with this how-to guide.

## Installation of `c8y_configuration_plugin`

To enable the feature, first you need to install the `c8y_configuration_plugin` binary on your device.

### With debian package

For Debian distribution OS, we provide the `c8y_configuration_plugin_<version>_<arch>.deb` package as a release asset [here](https://github.com/thin-edge/thin-edge.io/releases).

If you have used `get_thin_edge_io.sh` script to install `thin-edge.io`, this package is installed, by default.
You can skip this section.

In case that you didn't use the script, you can get the package on our [Releases](https://github.com/thin-edge/thin-edge.io/releases) and install it.

### For non-Debian OS distribution

To get the `c8y_configuration_plugin` binary, you can either extract it from the debian package or build from the sources.

#### Extracting from debian package

Get the `c8y_configuration_plugin_<version>_<arch>.deb` from our [Releases](https://github.com/thin-edge/thin-edge.io/releases).
Then, follow our guide [Extracting from debian package](./015_installation_without_deb_support.md#extracting-binaries-from-deb-packages).

#### Building from sources

Follow our guide [Building from source](./015_installation_without_deb_support.md#if-building-from-source).

A `systemd` unit file for `c8y_configuration_plugin` can be found in the repository at `configuration/init/systemd/c8y-configuration-plugin.service`
and should be installed on the target in: `/lib/systemd/system/c8y-configuration-plugin.service`.

## Get started

Before starting anything, first finish [establishing the connection to Cumulocity](./../tutorials/connect-c8y.md).

**Step 0**
Only if you didn't install `c8y_configuration_plugin` by the debian package,
you need one additional step to initialize the plugin. Run this command.

```shell
sudo c8y_configuration_plugin --init
```

**Step 1**
Modify the `/etc/tedge/c8y/c8y-configuration-plugin.toml` file in the [format written in the specification](./../references/c8y-configuration-management.md#configuration).

**Step 2**
Start the configuration plugin process and enable it on boot by `systemctl` (recommended).

```shell
sudo systemctl start c8y-configuration-plugin.service
sudo systemctl enable c8y-configuration-plugin.service
```

Alternatively, you can run the process directly.

```
sudo c8y_configuration_plugin
```

**Step 3**
Navigate to your Cumulocity Device Management and the desired device. Open its **Configuration** tab.
You can find `c8y-configuration-plugin` and more are listed as supported configuration types, as declared in the plugin configuration file in step 1.

![Cumulocity Configuration Management Upload](./images/c8y-config-plugin-upload.png)

This is the configuration file of `c8y_configuration_plugin`, where you can add file entries that you want to manage with Cumulocity.

## Update `c8y-configuration-plugin` from Cumulocity

To update any configuration file, create a local copy of that config file and then upload that file to the [Cumulocity configuration repository](https://cumulocity.com/guides/users-guide/device-management/#to-add-a-configuration-snapshot) with the appropriate configuration type.

The `c8y-configuration-plugin.toml` file can also be updated from the cloud in a similar manner to add/remove further configuration file entries. The updated TOML file has to be uploaded with the configuration type:  **c8y-configuration-plugin**.

Then, go back to the **Configuration** tab of your desired device in Cumulocity.

![Cumulocity Configuration Management Donwload](./images/c8y-config-plugin-download.png)

Click on the config file entry from the **DEVICE SUPPORTED CONFIGURATIONS** files list.
You can choose the file that you uploaded from the **AVAILABLE SUPPORTED CONFIGURATIONS** section, and then apply that file to your device by clicking on the **Send configuration to device** button.

After the operation created gets marked SUCCESSFUL, reload the page.
Then you can find new supported configuration types as you defined.

To get to know more about the `c8y_configuration_plugin`, refer to [Specifications of Device Configuration Management using Cumulocity](./../references/c8y-configuration-management.md).


