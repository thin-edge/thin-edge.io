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
Each configuration file is defined by a record with:
* The full `path` to the file.
* An optional configuration `type`. If not provided, the `path` is used as `type`.
* Optional unix file ownership: `user`, `group` and octal `mode`.  
  These are only used when a configuration file pushed from the cloud doesn't exist on the device.
  When a configuration file is already present on the device, this plugin never changes file ownership,
  ignoring these parameters.
* Optional `childid`. For details see section "Configuration files for child devices" below.
* Optional `desired`. For details see section "Configuration files for child devices" below.
* Optional `protocol`. Valid values are "http" or "filesystem". For details see section "Configuration files for child devices" below.

```shell
$ cat /etc/tedge/c8y/c8y-configuration-plugin.toml
files = [
    { path = '/etc/tedge/tedge.toml', type = 'tedge.toml' },
    { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf' },
    { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf' },
    { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto', user = 'mosquitto', group = 'mosquitto', mode = 0o644 }
  ]
```

Note that:
* The file `/etc/tedge/c8y/c8y-configuration-plugin.toml` itself doesn't need to be listed.
  This is implied, so the list can *always* be configured from the cloud.
  The `type` for this self configuration file is `c8y-configuration-plugin`.
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

## Configuration files for child devices

To manage configuration files for child-devices the `c8y_configuration_plugin` supports two aspects:
1) **Associating with cloud's child-device twin**<br/>
Allowing to associate a configuration file with a cloud's child-device twin
2) **Filetransfer from/to external device**<br/>
Allowing to consume/provide a configuration file from/to an external device via network

## Details to Aspect 1: Associating with cloud's child-device twin

For aspect (1) the plugin provides the field `childid` for all records in the `c8y_configuration_plugin` configuration (reference to section 'Configuration' above). That field is interpreted as unique child-device id and the plugin associates the record's configuration file with corresponding cloud's child-device twin. If not provided the configuration file is associated with cloud's thin-edge device twin.

Example:
```shell
$ cat /etc/tedge/c8y/c8y-configuration-plugin.toml
files = [
    { path = '/etc/tedge/tedge.toml', type = 'tedge.toml' },              # appears in the cloud on the thin-edge device
    { path = '/etc/child1/foo.conf', type = 'foo', childid = 'child1'  }  # appears in the cloud on the child-device 'child1'
  ]
```

**Declaration of capabilities in the cloud:**

For each cloud's child-device twin two files must be added under `/etc/tedge/operations/c8y/` (equal as for the thin-edge device twin it-self, see section "Installation" above). For child-devices these files will be to put into a subfolder, where the name of the subfolder is the `child-id`.

Example, for child-device with child-id `child1`:

```
/etc/tedge/operations/c8y/child1/c8y_UploadConfigFile
/etc/tedge/operations/c8y/child1/c8y_DownloadConfigFile
```

Note that the `c8y_configuration_plugin` does **not** create any child-device twin in the cloud. Instead the clouds child-device twins must be created upfront.

## Details to Aspect 2: Filetransfer from/to external device

For aspect (2) there are two proposals as below. Decision has to been taken which proposal to follow.

--------------------------------------------------------------------------------

**Proposal 1:**

The `c8y_configuration_plugin` configuration's record field `path` (reference to section 'Configuration' above) can be prefixed with "mqtt://". Then it's value is treated as MQTT topic structure, where another process/external device is expected to provide/consume the configuration file.

The `c8y_configuration_plugin` expects the process/external device always putting the latest config file as retain message to the MQTT topic `{path}`. Then the plugin consumes that retain message whenever a cloud request comes in.

Example Plugin Config:
```shell
$ cat /etc/tedge/c8y/c8y-configuration-plugin.toml
files = [
    { path = '/etc/tedge/tedge.toml', type = 'tedge.toml' },
    { path = 'mqtt://configs/bar.conf', type = 'bar.conf', childid = 'child1'  }
  ]
```

Example Flow:

**Start Behaviour:**
  * external device `child1`: starts
  * external device `child1`: publishs all its config files to MQTT (with retain); Example: Publish to topic `configs/bar.conf`

**Device-to-Cloud Behaviour:**
  * at some point a config retrieval for type `bar.conf` for `child1` arrives at C8Y config plugin
  * C8Y config plugin: subscribes to `configs/bar.conf` and gets immediately the config file content from the retained message
  * C8Y config plugin: sends the recived MQTT payload as config file to C8Y<br/><br/>
    NOTE: The responsibility to assure that the latest config file content is on the MQTT bus is always on the process/external device.
When there is no retain message the plugin sends an error to the cloud on an every incoming config retrieval request.

**Cloud-to-Device Behaviour:**
  * at some point a config sent from cloud for type `bar.conf` for `child1` arrives at C8Y config plugin
  * C8Y config plugin: publishs the config file content as MQTT retain message to `configs/bar.conf` 
  * external device `child1`: recognizes the MQTT message on `configs/bar.conf` and processes the new config file content

--------------------------------------------------------------------------------

**Proposal 3:**

To provide/consume configuration files to/from external devices, the HTTP filetransfer feature of the `tedge_agent` is used.
  
The HTTP filetransfer feature of the `tedge_agent` provides the service to transfer files from external devices to the local filesystem of the thin-edge device's, and vice versa.

  * `tedge_agent` serves PUT and GET requests on `http://<thin-edge IP address>/tedge/filetransfer/<path>`
    * `path` is chosen by the external-device; it may contain folders (e.g. `foo/bar/config-file.conf`)
    * on an incoming PUT request `tedge_agent` stores the contained file to `/http-root/<path>`
    * on an incoming GET request `tedge_agent` sends the file `/http-root/<path>` to the requester (i.E. the external device)
    * `tedge_agent` does not force any predefined folder structure within `/http-root/`. However for configuration management it is best practice to use folder structure as below:
      * `/http-root/config/current/<child-id>/<config filename>`<br/>
         for config-files provided by the external device to the thin-edge device (current state - or latest known state)<br/>
      * `/http-root/config/desired/<child-id>/<config filename>`<br/>
        for config-files to be consumed by the external device (desired state)
  * `tedge_agent` notifies over MQTT any change of a file under `/http-root/` (e.g. when uploaded by a external-device or updated by a local process). The notification messages are published on the topic `tedge/filetransfer_change/{path}`.<br/>
    TO-BE-DEFINED: Content of payload?
      
      Example:
      * HTTP PUT URL: `http://<thin-edge IP address>/tedge/filetransfer/config/current/child1/bar.conf`
      * relating MQTT notification topic: `/tedge/filetransfer_change/config/current/child1/bar.conf`

  * `c8y_configuration_plugin` allows to handle two versions for each configuration file (_current_ and _desired_).
    * The configuration file's record `path` is treated as path of the _current_ version of a configuration file (see to section 'Configuration' above).
    * The configuration file's record `desired` can be used to define the path of the _desired_ version of a configuration file. If `desired` is not defined, then `path` is treated as _desired_ version also.
    * On c8y read request for a configuration, `c8y_configuration_plugin` returns the `current` file content.
    * On c8y write request for a configuration, `c8y_configuration_plugin` updates the `desired` file content and waits for the `current` content to be updated to return the operation status to cumulocity.  
    * The current behavior of c8y_configuration_plugin can be simulated with desired = current = path when only a path is provided.

Example Plugin Config:
```shell
$ cat /etc/tedge/c8y/c8y-configuration-plugin.toml
files = [
    { type = 'bar.conf', child_id = 'child1', current = '/http-root/config/current/child1/bar.conf', desired = '/http-root/config/desired/child1/bar.conf' } }
  ]
```

Example Flow:

**Start Behaviour:**
  * external device child1: starts
  * external device child1: sends all its config files with HTTP PUT requests to the thin-edge device; 

    Example: HTTP PUT to `http://<thin-edge IP address>/tedge/filetransfer/current/child1/bar.conf`

**Device-to-Cloud Behaviour:**
  * at some point a config retrieval for type `bar.conf` for `child1` arrives at C8Y config plugin
  * C8Y config plugin: sends the file `/http-root/config/current/child1/bar.conf` to C8Y<br/><br/>
    NOTE: The responsibility to assure that the latest config file content on the thin-edge device is always on the external device. When there is no file the plugin sends an error to the cloud on an every incoming config retrieval request.

**Cloud-to-Device Behaviour:**
  * at some point a config sent from cloud for type `bar.conf` for `child1` arrives at C8Y config plugin
  * C8Y config plugin: stores the files received from C8Y to `/http-root/config/desired/child1/bar.conf`
  * C8Y config plugin: notifies the external device about the new configuration via MQTT (reference to section Notifications above)
  * external device child1: recognizes the MQTT notification and downloads the new config with an HTTP GET request
  
    Example: HTTP GET to `http://<thin-edge IP address>/tedge/filetransfer/config/desired/child1/bar.conf`
  * external device child1: after processing, it updates the _current_ configuration on the thin-edge device with an HTTP PUT request.

    Example: HTTP GET to `http://<thin-edge IP address>/tedge/filetransfer/config/current/child1/bar.conf`
  * C8Y config plugin: recognizes the MQTT notification about the updated file in the `current` folder,<br/>
    compares `/http-root/config/desired/child1/bar.conf` and `/http-root/config/current/child1/bar.conf`.
    * if both files are equal, the plugin reports success to C8Y.
    * if files are not equal, the plugin reports an error to C8Y.
    * TO-BE-DEFINED:
      * Which kind of error to report to C8Y in case of unequal files?
      * Any timeout to wait for update in folder `current`?

--------------------------------------------------------------------------------

