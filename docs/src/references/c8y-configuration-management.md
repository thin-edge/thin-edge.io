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
* Cumulocity manages the configuration files accordingly to their type,
  a name that is chosen by the device owner to categorise each configuration.
  By default, the full path of a configuration file on the device is used as its type.
* When files are downloaded from the cloud to the device,
  __these files are stored in a temporary directory first__.
  They are atomically moved to their target path, only after a fully successful download.
  The aim is to avoid breaking the system with half downloaded files.
* When a downloaded file is copied to its target, the unix user, group and mod are preserved.
* Once a snapshot has been downloaded from Cumulocity to the device,
  __the plugin publishes a notification message on the local thin-edge MQTT bus__.
  The device software has to subscribe to these messages if any action is required,
  say to check the content of file, to preprocess it or to restart a daemon.
* The configuration plugin also manage configuration files for child-devices connected to the main thin-edge device.
  From the cloud point of view, these child-devices are configured exactly using the same user interface,
  with the ability to focus on a device, to upload the current configuration files,
  to push configuration updates and to configure the list of configuration files.
  Behind the scene, the behavior is a bit more complex,
  the configuration plugin acting as a proxy between the cloud and the child-devices.
  The configuration updates are downloaded from the cloud on the thin-edge device
  then made available to the child-devices over HTTP,
  MQTT being used to notify the availability of these configuration updates.
  The child-device software has to subscribe to these messages, download the corresponding updates,
  and notify the main thin-edge configuration plugin of the update status.
  A similar combination of MQTT and HTTP is used to let the main device
  request a child device for a configuration file actually in use.
* In other words, the responsibilities of the plugin are:
  * to define the list of files under configuration management
  * to notify the cloud when this list is updated,
  * to upload these files to the cloud on demand,  
  * to download the files pushed from the cloud,
  * to make sure that the target files are updated atomically after successful download,
  * to notify the device software when the configuration is updated,
  * to act as proxy for the child-devices that need configuration management,
  * to publish over a local HTTP server the configuration files and make them available to the child-devices,
  * to notify the child-devices when configuration updates are available,
  * to notify the child-devices when current configuration files are requested from the cloud,
  * to consume over a local HTTP server the configuration files pushed by the child-devices.
* By contrast, the plugin is not responsible for:
  * checking the uploaded files are well-formed,
  * restarting the configured processes,
  * installing the configuration files on the child-devices.
* For each child-device, a device-specific software component is required
  to listen for configuration related MQTT notification
  and behave accordingly along the protocol defined by this configuration plugin.
  * Being specific to each type of child devices, this software has to be implemented specifically.
  * This software can be installed on the child device.
  * This software can also be installed on the main device,
    when the target device cannot be altered
    or connected to the main device over MQTT and HTTP.
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

The configuration plugin can also act as configuration proxy for child-devices.
For that to work for a child device named `$CHILD_DEVICE_ID`,
two files must be added under `/etc/tedge/operations/c8y/$CHILD_DEVICE_ID`
in order to declare the associated capabilities to Cumulocity.
These files are just empty files owned by the `tedge` user.

These two files can be created using the `c8y_configuration_plugin --init` option and providing child names:

```shell
$ sudo c8y_configuration_plugin --init child-1 child-2

$ ls -l /etc/tedge/operations/c8y/child-1
-rw-r--r-- 1 tedge tedge 97 Mar 22 14:24 /etc/tedge/operations/c8y/child-1/c8y_DownloadConfigFile
-rw-r--r-- 1 tedge tedge 95 Mar 22 14:24 /etc/tedge/operations/c8y/child-1/c8y_UploadConfigFile
  
$ ls -l /etc/tedge/operations/c8y/child-2
-rw-r--r-- 1 tedge tedge 97 Mar 22 14:24 /etc/tedge/operations/c8y/child-2/c8y_DownloadConfigFile
-rw-r--r-- 1 tedge tedge 95 Mar 22 14:24 /etc/tedge/operations/c8y/child-2/c8y_UploadConfigFile
```

The `c8y_configuration_plugin` has to be run as a daemon on the device, the latter being connected to Cumulocity.

On start of `tegde_mapper c8y` and on `/etc/tedge/operations/c8y` directory updates,
one can observe on the MQTT bus of the thin-edge device
the messages sent to Cumulocity to declare the capabilities of the main and child devices.
Here, the capabilities to upload and download configuration files
(possibly with other capabilities added independently):

```shell
$ tedge mqtt sub 'c8y/s/us/#'
[c8y/s/us] 114,c8y_Restart,c8y_SoftwareList,c8y_UploadConfigFile,c8y_DownloadConfigFile
[c8y/s/us/child-1] 114,c8y_UploadConfigFile,c8y_DownloadConfigFile
[c8y/s/us/child-2] 114,c8y_UploadConfigFile,c8y_DownloadConfigFile
```

## Configuration

The `c8y_configuration_plugin` configuration is stored by default under `/etc/tedge/c8y/c8y-configuration-plugin.toml`

This [TOML](https://toml.io/en/) file defines the list of files to be managed from the cloud tenant.
Each configuration file is defined by a record with:
* The full `path` to the file.
* An optional configuration `type`. If not provided, the `path` is used as `type`.
  This `type` is used to declare the configuration file to Cumulocity and then to trigger operations on that file.
  All the configuration `type`s for the main device are declared to the cloud on start
  and on change of the `c8y/c8y-configuration-plugin.toml` file.
* Optional unix file ownership: `user`, `group` and octal `mode`.  
  These are only used when a configuration file pushed from the cloud doesn't exist on the device.
  When a configuration file is already present on the device, this plugin never changes file ownership,
  ignoring these parameters.

```shell
$ cat /etc/tedge/c8y/c8y-configuration-plugin.toml
files = [
    { path = '/etc/tedge/tedge.toml', type = 'tedge.toml' },
    { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf' },
    { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf' },
    { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto', user = 'mosquitto', group = 'mosquitto', mode = 0o644 }
  ]
```

Along this `c8y_configuration_plugin` configuration for the main device,
the configuration plugin expects a configuration file per child device
that needs to be configured from the cloud.
* The configuration for a child-device `$CHILD_DEVICE_ID`
  is stored by default under `/etc/tedge/c8y/$CHILD_DEVICE_ID/c8y-configuration-plugin.toml`
* These TOML files have the same schema as for the main device,
  listing the configuration `files` and giving for each a `path` and possibly a `type`.
* Note that the `path` doesn't need to be a file path.
  It can be a key path in some registry of the child device or any name that makes sense for the child device.
* As for the main device, the `type` is used to name the configuration file on the cloud.
  All the configuration `type`s for a child devices are declared to the cloud on start
  and on change of the `c8y/$CHILD_DEVICE_ID/c8y-configuration-plugin.toml` file.
* The `user`, `group` and `mode` can be provided for a child-device configuration file,
  notably when used by the child device,
  but will not be used by the main device if provided.  

```shell
$ ls /etc/tedge/c8y/*/c8y-configuration-plugin.toml
/etc/tedge/c8y/child-1/c8y-configuration-plugin.toml 
/etc/tedge/c8y/child-2/c8y-configuration-plugin.toml

$ cat /etc/tedge/c8y/child-1/c8y-configuration-plugin.toml
files = [
    { path = '/var/camera.conf', type = 'camera' },
    { path = '/var/sounds.conf', type = 'sounds' },
  ]

$ cat /etc/tedge/c8y/child-2/c8y-configuration-plugin.toml
files = [
    { path = '/var/ai/model' },
  ]
```

On start and when one of these files is updated, the configuration plugin sends
one [`119 SmartRest2 message`](https://cumulocity.com/guides/10.14.0/reference/smartrest-two/#119) per device
to Cumulocity with the set of `type`s listed by the configuration
(adding implicitly the `c8y-configuration-plugin` themselves).
These messages can be observed over the MQTT bus of the thin-edge device.
In the case of the example, 3 messages are sent - one for the main device and 2 for the child devices:

```shell
$ tedge mqtt sub 'c8y/s/us/#'
[c8y/s/us] 119,c8y-configuration-plugin,tedge.toml,/etc/tedge/mosquitto-conf/c8y-bridge.conf,/etc/tedge/mosquitto-conf/tedge-mosquitto.conf,mosquitto
[c8y/s/us/child-1] 119,c8y-configuration-plugin,camera,sounds
[c8y/s/us/child-2] 119,c8y-configuration-plugin,/var/ai/model
```

Note that:
* The file `/etc/tedge/c8y/c8y-configuration-plugin.toml` itself doesn't need to be listed.
  This is implied, so the list can *always* be configured from the cloud.
  The `type` for this self configuration file is `c8y-configuration-plugin`.
* If the file `/etc/tedge/c8y/c8y-configuration-plugin.toml`
  is not found, empty, ill-formed or not-readable
  then only `c8y-configuration-plugin.toml` is managed from the cloud.
* Similarly, when there is a directory `/etc/tedge/c8y/$CHILD_DEVICE_ID/`
  but the file `/etc/tedge/c8y/$CHILD_DEVICE_ID/c8y-configuration-plugin.toml`
  is not found , empty, ill-formed or not-readable
  then only `$CHILD_DEVICE_ID/c8y-configuration-plugin.toml` is managed from the cloud.
* If the file `/etc/tedge/c8y/c8y-configuration-plugin.toml` is ill-formed
  or cannot be read then an error is logged, but the operation proceed
  as if the file were empty.
  Similarly, for any file `/etc/tedge/c8y/$CHILD_DEVICE_ID/c8y-configuration-plugin.toml`.
  So, the issue can be fixed from the cloud.
  
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
        --debug                        Turn-on the debug log level
    -h, --help                         Print help information
    -i, --init [CHILD_DEVICE_ID]       Create supported operation files, possibly for several child devices
    -V, --version                      Print version information

    On start, `c8y_configuration_plugin` notifies the cloud tenant of the managed configuration files,
    listed in `CONFIG_DIR/c8y/c8y-configuration-plugin.toml`, sending this list with a `119` on `c8y/s/us`.
    `c8y_configuration_plugin` subscribes then to `c8y/s/ds` listening for configuration operation
    requests (messages `524` and `526`).
    notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).
    
    The thin-edge `CONFIG_DIR` is used to find where:
    * to store the configuration file: `c8y/c8y-configuration-plugin.toml`
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

* The notification messages are published on the topic `tedge/configuration_change/{type}`,
  where `{type}` is the type of the configuration file that have been updated,
  for instance `tedge/configuration_change/tedge.toml`
* Each message provides the path to the freshly updated file as in `{ "path": "/etc/tedge/tedge.toml" }`.

Note that:
* If no specific type has been assigned to a configuration file, then the path to this file is used as its type.
  Update notifications for that file are then published on the topic `tedge/configuration_change/{path}`,
  for instance `tedge/configuration_change//etc/tedge/mosquitto-conf/c8y-bridge.conf`.
* Since the type of configuration file is used as an MQTT topic name, the characters `#` and `+` cannot be used in a type name.
  If such a character is used in a type name (or in the path of a configuration file without explicit type),
  then the whole plugin configuration `/etc/tedge/c8y/c8y-configuration-plugin.toml` is considered ill-formed.

## Configuration protocol between thin-edge and the child-devices

The configuration plugin `c8y_configuration_plugin` can act as a proxy between the cloud and a child-device.
However, for that to work, a client must be installed on the child device
to perform the actual configuration updates pushed by thin-edge on behalf of the cloud.
While the configuration plugin tells what need to be updated and when,
only the child device specific client can control where and how these updates can be applied.

* The responsibility of the configuration plugin is to
  * interact with the cloud, receiving the configuration update and configuration snapshot requests,
  * exchange configuration files with the child device via an HTTP-based file transfer service over the local network,
  * notify the child devices via MQTT when configuration files are to be updated or requested from the cloud,
  * listen to child devices' configuration operation status via MQTT messages and mirror those to the cloud.
* The child-device configuration software is an MQTT+HTTP client that
  * interact with the child-device system, accessing the actual configuration files
  * connect the main thin-edge device over the local MQTT bus,
  * listen over MQTT for configuration updates and requests,
  * download and upload the configuration files on demand,
  * notify the progress of the configuration operations to the main device via MQTT.

For each kind of child device such a specific client has to be implemented and installed,
most often on the child-device hardware but not necessarily as one can imagine a process running
thin-edge and acting as a proxy for a child device which software cannot be altered.

Here is the protocol that has to be implemented by the child-device configuration client.
This protocol covers 4 interactions, the child devices:
1. Connecting to thin-edge
1. Downloading configuration file updates from thin-edge
1. Uploading current configuration files to thin-edge
1. Notifying the list of configuration files to thin-edge

