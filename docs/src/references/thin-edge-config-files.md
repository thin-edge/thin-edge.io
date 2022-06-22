# Thin-edge config files

Thin-edge.io requires config files for its operation. The `tedge --init` option is used to create
the base directory and other directories inside the base directory with appropriate user and permissions.
The `tedge_mapper --init c8y/az` and `tedge_agent --init` will create the
directories/files inside the base directory that are required for their operation.

By default, the config files are created in `/etc/tedge` directory. To create the config files in 
a custom base directory one has to use `--config-dir <Path to base directory>` option.

## Creating `thin-edge` config files

The config files are created using `tedge --init` as below.

```shell
$ sudo tedge --init
```

All the directories will be created in the `/etc/tedge` directory. The directories layout looks as below.

```shell
$ ls -l /etc/tedge
total 16
drwxrwxr-x 2 mosquitto mosquitto 4096 Jun 10 14:49 device-certs
drwxrwxr-x 2 tedge     tedge     4096 Jun 10 14:49 mosquitto-conf
drwxrwxr-x 2 tedge     tedge     4096 Jun 10 14:49 operations
drwxrwxr-x 2 tedge     tedge     4096 Jun 10 14:49 plugins
```

Use the below command to create the config directories in a custom directory.

```shell
$ sudo tedge --config-dir /global/path/to/config/dir --init
```

Now all the config directories will be created inside the `/global/path/to/config/dir` directory.


The directories and files that are required by the `tedge_mapper` are created as below.

```shell
$ sudo tedge_mapper --init c8y

$ ls -l /etc/tedge/operations/c8y
total 0
-rw-r--r-- 1 tedge tedge 0 Jun 14 14:37 c8y_Restart
-rw-r--r-- 1 tedge tedge 0 Jun 14 14:37 c8y_SoftwareUpdate
```
To create these directories in a custom directory, use `--config-dir` option as below.

```shell
$ sudo tedge_mapper --config-dir /global/path/to/config/dir --init c8y
```

The directories and files that are required by the `tedge_agent` are created as below.

```shell
$ sudo tedge_agent --init

$ ls -l /etc/tedge/.agent
-rw-r--r-- 1 tedge tedge 0 Jun 15 11:51 /etc/tedge/.agent/current-operation
```
To create these directories and files in a custom directory, use the `--config-dir` option as below as below.

```shell
$ sudo tedge_agent --config-dir /global/path/to/config/dir --init
```

## Manage the configuration parameters

The configuration parameters can be set/unset/list in a config file as below

For example, the config parameter can be set as below.

```shell
$ sudo tedge config set c8y.url your.cumulocity.io
```
Now the configuration will be added into `/etc/tedge/tedge.toml`

Use the below command to set/unset/list configuration parameters in a config file that is present
in a custom directory.

```shell
$ sudo tedge --config-dir /global/path/to/config/dir config set c8y.url your.cumulocity.io
```

Now the config will be set in `/global/path/to/config/dir/tedge/tedge.toml`

## Manage the certificate

To create/remove/upload the certificate, one can use the below command.

```shell
$ sudo tedge cert create --device-id thinedge

# Find the certificates that are created as below.

$ ls -l /etc/tedge/device-certs/
total 8
-r--r--r-- 1 mosquitto mosquitto 638 Jun 14 14:38 tedge-certificate.pem
-r-------- 1 mosquitto mosquitto 246 Jun 14 14:38 tedge-private-key.pem
```

Use the below command to create/remove/upload the certificate.

```shell
$ sudo tedge --config-dir /global/path/to/config/dir cert create --device-id thinedge

# Find the certificates that are created as below.

$ ls -l /global/path/to/config/dir/tedge/device-certs/
total 8
-r--r--r-- 1 mosquitto mosquitto 638 Jun 14 14:38 tedge-certificate.pem
-r-------- 1 mosquitto mosquitto 246 Jun 14 14:38 tedge-private-key.pem
```

## Connecting to the cloud

Use the` tedge connect c8y/az` command to connect to the cloud using the default configuration files
that are present in `/etc/tedge`.

To connect to the cloud with config files that are present in a custom location use
the `tedge connect --config-dir <Path to custom dir> c8y/az` option.

This is a two step process.

### Step 1: Update the `mosquitto.conf`

Since the bridge configuration files for Cumulocity IoT or Azure IoT Hub will be created in a directory given through `--config-dir`,
the path to the bridge configuration files (tedge-mosquitto.conf, c8y/az-bridge.conf) must be found by `mosquitto`.
So, the below line has to be added to your `mosquitto.conf` file manually.

```include_dir /global/path/to/config/dir/tedge/mosquitto-conf```

### Step 2: `tedge connect <cloud[c8y/az]>` using the `--config-dir` option

Use the below command to connect to `Cumulocity IoT or Azure IoT Hub` cloud using `--config-dir`

```shell
$ sudo tedge --config-dir /global/path/to/config/dir connect c8y/az
```

Here the `path/to/config/dir` is the directory where the configuration files are present.

