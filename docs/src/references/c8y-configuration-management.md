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

For aspect (1) the external device sends an MQTT message to `tedge/meta/plugin/configuration/<childid>` to announce it's configuration management capability to thin-edge. That MQTT message contains all configurations the external device provides. Thereby each configuration appears with a `type` and with an optional field `path`. If path is specified, the `c8y_configuration_plugin` consumes/provides the configuration-file from/to the given local filesystem path. If the field `path` is not given, the `c8y_configuration_plugin` make use of the HTTP filetransfer feature of the `tedge_agent` to consume/provide the configuration-file (see [section below](#details-to-aspect-2-filetransfer-fromto-external-device) for more details about HTTP filetransfer). The first case is intended for local processes (running on the thin-edge device) that represent a child-device, and the latter case is intended for external devices.

The MQTT message is as below:

```json
{
   "configurations": [
     {
       "type": "<config type 1>",
       "path": "</path/to/file/file 1>"
     },
     {
       "type": "<config type 2>",
       "path": "</path/to/file/file 2>"
     },
     ...
   ]
}
```

Example:
```json
{
   "configurations": [
     {
       "type": "foo.conf",
       "path": "/etc/child1/foo.conf"
     },
     {
       "type": "bar.conf",
       "path": "/etc/child1/bar.conf"
     }
   ]
}
```

Each time the `c8y_configuration_plugin` receivces that message, it takes care to define all necessary capabilities to the coresponding cloud's child-device twin. These are:
  - declaring _supported operations_ for configuration management: `c8y_UploadConfigFile` and `c8y_DownloadConfigFile`
  - declaring provided _configuration types_

**Declaring 'supported operations'**

To declare supported operations the `c8y_configuration_plugin` uses thin-edge's _Supported Operations API_. Therefore the `c8y_configuration_plugin` creates for each child-device two files under `/etc/tedge/operations/c8y/<childid>`.

Example, for child-device with childid `child1`:

```
/etc/tedge/operations/c8y/child1/c8y_UploadConfigFile
/etc/tedge/operations/c8y/child1/c8y_DownloadConfigFile
```

As soon as those files are created, thin-edge's _Supported Operations API_ takes care to send according supported operation declarations to cloud's child-device twins (see [documentation Supported Operations](../tutorials/supported_operations.md#supported-operations-for-child-devices)).

**Declaring 'provided configuration types'**

For all configuration `types` provided by the external device, the plugin sends an MQTT message to C8Y. Thereby all `types` will be combined in one single message as below:
  - topic: `c8y/s/us/<childid>`
  - payload: `119,<type 1>,<type 2>,<type 3>,...`<br/>
    Example: `119,foo.conf,bar.conf`

Note that the `c8y_configuration_plugin` does **not** create any child-device twin in the cloud. Instead the clouds child-device twins must be created upfront.


## Details to Aspect 2: Filetransfer from/to external device

To provide/consume configuration files to/from external devices, the `c8y_configuration_plugin` make use of the HTTP filetransfer feature of the `tedge_agent` is used.
  
The HTTP filetransfer feature of the `tedge_agent` provides the service to transfer files from external devices to the local filesystem of the thin-edge device's, and vice versa.

  * `tedge_agent` serves PUT and GET requests for temporary file up/downloads
    * a temporary file can be uploaded with an HTTP PUT request to `http://<thin-edge IP address>/tedge/tmpfiles`,
      where the HTTP response contains a _temporary URL_.
    * a temporary file can be downloaded with an HTTP GET request to `http://<ip address of thin-edge device>/<temporary URL>`
    * a local API allows the plugin to access (expose and obtains) a temporary file directly via the thin-edge device's local filesystem

    * TODO: Investigate and decide about filetransfer's local API in scheduled prototype (https://github.com/thin-edge/thin-edge.io/issues/1307), according to topics below:
 
      * local API must avoid that there are two copies of the same file at any point in time
      * local API must avoid that the file content is transfered/copied from one to another place, when the plugin _obtains_ the file. 
        Instead just the directory link may change (as it is the case for bash `mv` command).

Example Flow:

**Fetch configuration file from child device to cloud**
  * at some point a config retrieval for type `bar.conf` for `child1` arrives at C8Y config plugin<br/>
    Format of C8Y SmartREST message for config retrieval operation: `526,<childid>,<config type>`. See [C8Y SmartREST doc](https://cumulocity.com/guides/reference/smartrest-two/#upload-configuration-file-with-type-526)<br/>
    Example: `526,child1,bar.conf`

  * C8Y config plugin: notifies the external device `child1` via MQTT to upload it's current configuration to the thin-edge device

    Topic: `tedge/configuration/req/retrieve/{config type}/{childid}`<br/>
    Example: `tedge/configuration/req/retrieve/bar.conf/child1`<br/>  
    TODO: Investigate and decide about topic structure and payload in scheduled prototype (https://github.com/thin-edge/thin-edge.io/issues/1307) 
         
  * external device child1: Uploads configuration file to the thin-edge device with the HTTP filetransfer feature.
  
    HTTP PUT request: `http://<ip address of thin-edge devicee>/tedge/tmpfiles`
    
    The response from the HTTP filetransfer feature contains a _temporary URL_, that is later used to uniquely access that uploaded file.<br/>
    Example for a _temporary URL_: `/tedge/tmpfiles/<random string>`

  * external device child1: notifies the the plugin via MQTT about succeeded upload and the _temporary URL_.
 
    Topic: `tedge/configuration/res/retrieve/{config type}/{childid}`<br/>
    Example: `tedge/configuration/res/retrieve/bar.conf/child1`<br/>
    Payload: ` <temporary URL> `
       
    TODO: Investigate and decide about topic structure and payload in scheduled prototype (https://github.com/thin-edge/thin-edge.io/issues/1307). Unhappy paths also to be considered here.
  
  * C8Y config plugin: recognizes the MQTT notification about the uploaded file and the temprary-URL, 
                       and uses the filetransfers local API to obtain the file from the _temprary URL_ to some plugin specific local filesystem path (e.g. `/tmp/c8y-cfg-plugin/<childid>_<cfg-type>`.

    * TODO: Investigate and decide about filetransfer's local API in scheduled prototype (https://github.com/thin-edge/thin-edge.io/issues/1307), according to topics below:
 
      * local API must avoid that there are two copies of the same file at any point in time
      * local API must avoid that the file content is transfered/copied from one to another place, when the plugin _obtains_ the file. 
        Instead just the directory link may change (as it is the case for bash `mv` command).
      * file is removed from filetransfer's specific folder when the file was successfully obtained to the plugin's path.
      
      Reasons for requirements above: Saving disc-space and CPU resources for large files.

    * TO-BE-DEFINED:
    
      * Decide which error paths to be considered.
      * Decide if any timeout to be considerd when waiting for upload notification from external device.

  * C8Y config plugin: sends the obtained file to C8Y

  * C8Y config plugin: removed the file form local filesystem
  

**Push configuration file update from cloud to child device**
  * at some point a config sent from cloud for type `bar.conf` for `child1` arrives at C8Y config plugin<br/>
    Format of C8Y SmartREST message for config send operation: `524,<childid>,<URL>,<config type>`. See [C8Y SmartREST doc](https://cumulocity.com/guides/reference/smartrest-two/#download-configuration-file-with-type-524)<br/>
    Example: `524,child1,http://www.my.url,bar.conf`
  * C8Y config plugin: downloads the file based on the URL received from C8Y, and stores it some plugin specific local filesystem path (e.g. `/tmp/c8y-cfg-plugin/<childid>_<cfg-type>`).

  * C8Y config plugin: uses the filetransfers local API to expose the file to the filetransfer feature. The filetransfer feature responses a _temporary URL_, that is later used to uniquely access that exposed file.

    Example for a _temporary URL_: `/tedge/tmpfiles/<random string>`

    * TODO: Investigate and decide about filetransfer's local API in scheduled prototype (https://github.com/thin-edge/thin-edge.io/issues/1307), according to topics below:
 
      * local API must avoid that there are two copies of the same file at any point in time
      * local API must avoid that the file content is transfered/copied from one to another place, when the plugin _exposes_ the file. 
        Instead just the directory link may change (as it is the case for bash `mv` command).
      * file is removed from plugin's source folder, when the file was successfully exposed.
      
      Reasons for requirements above: Saving disc-space and CPU resources for large files.

  * C8Y config plugin: notifies the external device `child1` via MQTT to download the new configuration from the thin-edge device with the _temporary URL_.

    Topic: `tedge/configuration/req/send/{config type}/{childid}`<br/>
    Example: `tedge/configuration/req/send/bar.conf/child1`<br/>  
    Payload: ` <temporary URL> `<br/>
    TODO: Investigate and decide about topic structure and payload in scheduled prototype (https://github.com/thin-edge/thin-edge.io/issues/1307) 

  * external device child1: Downloads configuration file from the thin-edge device with the HTTP filetransfer feature and the _temporary URL_, and filetransfer removes the file from it's local filesystem folder.
  
    Example for HTTP GET request: `http://<ip address of thin-edge device>/<temporary URL>`
    
  * external device child1: applies the new configuration and notifies the the plugin via MQTT about success.
 
    Topic: `tedge/configuration/res/send/{config type}/{childid}`<br/>
    Example: `tedge/configuration/res/send/bar.conf/child1`<br/>
       
    TODO: Investigate and decide about topic structure and payload in scheduled prototype (https://github.com/thin-edge/thin-edge.io/issues/1307). Unhappy paths also to be considered here.

  * C8Y config plugin: recognizes the MQTT notification and sends the result to C8Y.

    TO-BE-DEFINED:
    
      * Decide which error paths to be considered.
      * Decide if any timeout to be considerd when waiting for upload notification from external device.


## Notfifications for Child-Devices

Notifications for child-devices behave the same way as nofifications for the thin-edge device (as described in section _Notifications_ above), but are extended with the `childid`. That is since the `type` is unique for one device, but can again occur on another child-device or the thin-edge device.

Therefore the topic child-device notifications are published to has the `childid` appended: `tedge/configuration_change/{type}/{childid}`,
  where `{type}` is the type of the configuration file that has been updated,
  for instance `configs/bar.conf`
  
  and `{childid}` is the child-device id of the configuration file that has been updated,
  for instance `child1`
  
Everything else about child-device notifications behaves in the same way as for the thin-edge device.
