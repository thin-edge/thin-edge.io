# How to manage configuration files with Cumulocity

With `thin-edge.io`, you can manage files on a device by using the Cumulocity's feature,
[Managing Configurations](https://cumulocity.com/guides/users-guide/device-management/#managing-configurations) as a part of Device Management.

If you are new to the Cumulocity **Management Configurations** feature,
we recommend you to read [the Cumulocity user guide](https://cumulocity.com/guides/users-guide/device-management/#managing-configurations) along with this how-to guide.

## Installation of `c8y_configuration_plugin`

To enable the feature, first you need to install the `c8y_configuration_plugin` binary on your device.

### With debian package

For Debian distribution OS, we provide the `c8y_configuration_plugin_<version>_<arch>.deb` package.

If you have used `get_thin_edhe_io.sh` script to install `thin-edge.io`, your device has already installed the package.
You can skip this section.

In case that you didn't use the script, you can get the package on our [Releases](https://github.com/thin-edge/thin-edge.io/releases) and install it.

### For no Debian distribution OS

To place the binary `c8y_configuration_plugin` on your device,
you can choose either extracting the binary from the debian package or building from the sources.

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
Start the configuration plugin process by `systemctl` (recommended).

```shell
sudo systemctl start c8y-configuration-plugin.service
```

Alternatively, you can run the process directly.

```
sudo c8y_configuration_plugin
```

**Step 3**
Navigate to your Cumulocity Device Management and the desired device. Open its **Configuration** tab.
You can find `c8y-configuration-plugin` and more are declared as supported configuration types according to the plugin configuration by step 1.

![Cumulocity Configuration Management Upload](./images/c8y-config-plugin-upload.png)

This is the configuration file of `c8y_configuration_plugin`, where you can add file entries that you want to manage with Cumulocity.

## Update `c8y-configuration-plugin` from Cumulocity

To update the `c8y-configuration-plugin`,
prepare a TOML file in the [format written in the specification](./../references/c8y-configuration-management.md#configuration) on your local machine.
Then, upload the file in the [Configuration repository](https://cumulocity.com/guides/users-guide/device-management/#to-add-a-configuration-snapshot) with the configuration type **c8y-configuration-plugin**.

Then, go back to the **Configuration** tab of your desired device in Cumulocity.

![Cumulocity Configuration Management Donwload](./images/c8y-config-plugin-download.png)

Click `c8y-configuration-plugin` under the supported configurations.
You can choose the file that you uploaded, then apply the file to your device.

After the operation created gets marked SUCCESSFUL, reload the page.
Then you can find new supported configuration types as you defined.

To get to know more about the `c8y_configuration_plugin`, refer to [Specifications of Device Configuration Management using Cumulocity](./../references/c8y-configuration-management.md).


