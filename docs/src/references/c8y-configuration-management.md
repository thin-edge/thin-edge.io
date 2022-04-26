# Device Configuration Management using Cumulocity

Thin-edge provides an operation plugin to
[manage device configurations using Cumulocity](https://cumulocity.com/guides/users-guide/device-management/#to-retrieve-and-apply-a-configuration-snapshot-to-a-device-which-supports-multiple-configuration-types).

* This management is bi-directional:
  * A device can be taken as reference, all the managed files being uploaded to the cloud
    and stored there as a configuration snapshot.
  * A configuration snapshot can be pushed from the cloud to any devices of the same type,
    i.e. supporting the same kind of configuration files.
* With this operation plugin, the device owner defines the list of files
  (usually configuration files, but not necessarily),
  that will be managed from the cloud tenant.
* Notably, __the plugin configuration itself is managed from the cloud__,
  meaning, the device owner can update from the cloud the list of files to be managed.
* By default, __the managed files are represented on the cloud by their full path on the device__.
  Optionally, the device owner can assign an alias to each target path.  
* When files are downloaded from the cloud to the device,
  __these files are stored in a temporary directory first__.
  They are atomically moved to their target path, only after a fully successful download.
  The aim is to avoid breaking the system with half downloaded files.
* When a downloaded file is copied to its target, the unix user, group and mod are preserved.
* Once a snapshot has been downloaded from Cumulocity to the device,
  __the plugin publishes a notification message on the local thin-edge MQTT bus__.
  The device software has to subscribe to these messages if any action is required,
  say to check the content of file, to preprocess it or to restart a daemon. 
* In other words, the responsibilities of the plugin are:
  * to define the list of files under configuration management
  * to notify the cloud when this list is updated,
  * to upload these files to the cloud on demand,  
  * to download the files pushed from the cloud,
  * to make sure that the target files are updated atomically after successful download,
  * to notify the device software when the configuration is updated.
* By contrast, the plugin is not responsible for:
  * checking the uploaded files are well-formed,
  * restarting the configured processes.
* A user-specific component, installed on the device,
  can implement more sophisticated configuration use-cases by:
  * listening for configuration updates on the local thin-edge MQTT bus,
  * restarting the appropriate processes when appropriate,  
  * declaring intermediate files as the managed files,
    to have the opportunity to check or update their content
    before moving them to the actual targets.

## Installation

Assuming the configuration plugin `c8y_configuration_plugin`
has been installed in `/usr/bin/c8y_configuration_plugin`,
two files must be added under `/etc/tedge/operations/c8y/`
to declare that this plugin supports two Cumulocity operations:
uploading and downloading configuration files
(which respective SmartRest2 codes are `526` and `524`).

These two files can be created using the `c8y_configuration_plugin --init` option:

```shell
$ sudo c8y_configuration_plugin --init

$ ls -l /etc/tedge/operations/c8y/c8y_UploadConfigFile
-rw-r--r-- 1 tedge tedge 95 Mar 22 14:24 /etc/tedge/operations/c8y/c8y_UploadConfigFile
  
$ ls -l /etc/tedge/operations/c8y/c8y_DownloadConfigFile
-rw-r--r-- 1 tedge tedge 97 Mar 22 14:24 /etc/tedge/operations/c8y/c8y_DownloadConfigFile
```

The `c8y_configuration_plugin` has to be run as a daemon on the device, the latter being connected to Cumulocity.

## Configuration

The `c8y_configuration_plugin` configuration is stored by default under `/etc/tedge/c8y/c8y-configuration-plugin.toml`

This [TOML](https://toml.io/en/) file defines the list of files to be managed from the cloud tenant.

```shell
$ cat /etc/tedge/c8y/c8y-configuration-plugin.toml
files = [
    '/etc/tedge/tedge.toml',
    '/etc/tedge/mosquitto-conf/c8y-bridge.conf',
    '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf',
    '/etc/mosquitto/mosquitto.conf'
  ]
```

Note that:
* The file `/etc/tedge/c8y/c8y-configuration-plugin.toml` itself doesn't need to be listed.
  This is implied, so the list can *always* be configured from the cloud.
* If the file `/etc/tedge/c8y/c8y-configuration-plugin.toml`
  is not found, empty, ill-formed or not-readable
  then only `c8y-configuration-plugin.toml` is managed from the cloud.
* If the file `/etc/tedge/c8y/c8y-configuration-plugin.toml` is ill-formed
  or cannot be read then an error is logged, but the operation proceed
  as if the file were empty.
  
The behavior of the `c8y_configuration_plugin` is also controlled
by the configuration of thin-edge:

* `tedge config get mqtt.bind_address`: the address of the local MQTT bus.
* `tedge config get mqtt.port`: the TCP port of the local MQTT bus.
* `tedge config get tmp.path`: the directory where the files are updated
  before being copied atomically to their targets.

## Usage

```shell
$ c8y_configuration_plugin --help
c8y_configuration_plugin 0.6.2
Thin-edge device configuration management for Cumulocity

USAGE:
    c8y_configuration_plugin [OPTIONS]

OPTIONS:
        --config-dir <CONFIG_DIR>      [default: /etc/tedge]
        --config-file <CONFIG_FILE>    [default: $CONFIG_DIR/c8y/c8y-configuration-plugin.toml]
        --debug                        Turn-on the debug log level
    -h, --help                         Print help information
    -i, --init                         Create supported operation files
    -V, --version                      Print version information

    On start, `c8y_configuration_plugin` notifies the cloud tenant of the managed configuration files,
    listed in the `CONFIG_FILE`, sending this list with a `119` on `c8y/s/us`.
    `c8y_configuration_plugin` subscribes then to `c8y/s/ds` listening for configuration operation
    requests (messages `524` and `526`).
    notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).
    
    The thin-edge `CONFIG_DIR` is used to find where:
    * to store temporary files on download: `tedge config get tmp.path`,
    * to connect the MQTT bus: `tedge config get mqtt.port`.
```

## Logging

The `c8y_configuration_plugin` reports progress and errors on its `stderr`.

* All upload and download operation requests are logged, when received and when completed,
  with one line per file.
* All changes to the list of managed file is logged, one line per change.
* All errors are reported with the operation context (upload or download? which file?).

## Notifications

When a configuration file is successfully downloaded from the cloud,
the `c8y_configuration_plugin` service notifies this update over MQTT.

* Each message provides the path to the freshly updated file as in `{ "changedFile": "/etc/tedge/tedge.toml" }`.
* The messages are published on `tedge/configuration_change`.

## Internals

Points that still need to be addressed:

* Ability to give a type name to a configuration file.
* What if the target file doesn't exist? Is this is an error? If not what user, group and mod are to be used?