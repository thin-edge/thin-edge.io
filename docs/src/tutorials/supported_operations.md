# thin-edge.io Supported Operations

## Supported Operations concepts

### Device operations

IoT devices often do more than just send data to the cloud. They also do things like:

* receive triggers from the operator
* reboot on demand
* install or remove software

These operations are supported by [Cumulocity IoT](https://cumulocity.com/api/10.11.0/#section/Device-management-library) and other cloud providers.
On `thin-edge.io` the support for one such operation can be added using the `thin-edge.io` Supported Operations API.

### `thin-edge.io` Supported Operations API

The Supported Operations utilises the file system to add and remove operations. A special file placed in `/etc/tedge/operations` directory will indicate that an operation is supported.
The specification for the operation files is described in `thin-edge.io` specifications repository[src/supported-operations/README.md](https://github.com/thin-edge/thin-edge.io-specs/blob/main/src/supported-operations/README.md)

Supported operations are declared in the cloud specific subdirectory of `/etc/tedge/operations` directory.

## Custom Supported Operations

`thin-edge.io` supports custom operations by using configuration files and plugin mechanism similar to what software management agent does.

The main difference between custom operations and native operations is that custom operations are have to be defined in configuration files and provide their own implementation in a callable `plugin` executable.
As per specification the configuration file needs to be a `toml` file which describes the operation.

`thin-edge.io` stores the operations configuration files in the `/etc/tedge/operations/<cloud-provider>/` directory.

## `thin-edge.io` List of Supported Operations

`thin-edge.io` supports natively the following operations:

* Software Update
* Software Update Log Upload
* Restart

The list is growing as we support more operations, but is not exhaustive and we encourage you to contribute to the list.

## How to use Supported Operations

### Listing current operations

You can obtain the current list of supported operations by listing the content of the `/etc/tedge/operations` directory.
This directory should have permissions set to `755` and the owner to `tedge`.
This directory will contain a set subdirectories based on cloud providers currently supported eg:

```shell
$ ls -l /etc/tedge/operations

drwxr-xr-x 2 tedge tedge 4096 Jan 01 00:00 az
drwxr-xr-x 2 tedge tedge 4096 Jan 01 00:00 c8y
```

From the above you can see that there are two cloud providers supported by `thin-edge.io`.
The directories should be readable by `thin-edge.io` user - `tedge` - and should have permissions `755`.

To list all currently supported operations for a cloud provider, run:

```shell
$ ls -l /etc/tedge/operations/c8y

-rw-r--r-- 1 tedge tedge 0 Jan 01 00:00 c8y_Restart
```

To list all currently supported operations, run:
The operations files should have permissions `644` and the owner `tedge`.

```shell
$ sudo ls -lR /etc/tedge/operations
/etc/tedge/operations:
drwxr-xr-x 2 tedge tedge 4096 Jan 01 00:00 az
drwxr-xr-x 2 tedge tedge 4096 Jan 01 00:00 c8y

/etc/tedge/operations/az:
-rw-r--r-- 1 tedge tedge 0 Jan 01 00:00 Restart

/etc/tedge/operations/c8y:
-rw-r--r-- 1 tedge tedge 0 Jan 01 00:00 c8y_Restart
```

### Adding new operations

To add new operation we need to create new file in `/etc/tedge/operations` directory.
Before we create that file we have to know which cloud provider we are going to support (it is possible to support multiple cloud providers, but we won't cover this here).

We will add operation `Restart` for our device which can be triggered from Cumulocity IoT called, in Cumulocity IoT this operations name is `c8y_Restart`.
This operation will do the reboot of our device when we receive trigger from the operator. `thin-edge.io` device will receive an MQTT message with certain payload and we already have a handler for that payload in the `tedge-mapper-c8y`.

To add new operation we will create a file in `/etc/tedge/operations/c8y` directory:

```shell
sudo -u tedge touch /etc/tedge/operations/c8y/c8y_Restart
```

> Note: We are using `sudo -u` to create the file because we want to make sure that the file is owned by `tedge` user.

Now the new operation will be automatically added to the list and the list will be sent to the cloud.

### Removing supported operations

To remove supported operation we can remove the file from `/etc/tedge/operations/c8y` directory. eg:

```shell
sudo rm /etc/tedge/operations/c8y/c8y_Restart
```

Now the operation will be automatically removed from the list and the list will be sent to the cloud.

## Working with custom operations

We will use the `thin-edge.io` Supported Operations API to add custom operations. Our new operation is going to be capability to execute shell commands on the device.
Let's create the operation configuration file:

We need to tell `thin-edge.io` how to handle the operation and how to execute it.

### Adding new custom operation

In Cumulocity IoT we know that there is an operation call c8y_Command which allows us to send commands to the device and get the result back to the cloud, let's create the configuration file for our new operation:

First we create a file with the name of the operation:

```shell
sudo -u tedge touch /etc/tedge/operations/c8y/c8y_Command
```

> Note: the needs to be readable by `thin-edge.io` user - `tedge` - and should have permissions `644`.

In this example we want `thin-edge.io` to pick up a message on specific topic and execute the command on the device, our topic is `c8y/s/ds`.
We also know that the message we expect is going to use SmartRest template `511` and our plugin is located in `/etc/tedge/operations/command`.
Then we need to add the configuration to the file (`/etc/tedge/operations/c8y/c8y_Command`):

```toml
[exec]
  topic = "c8y/s/ds"
  on_message = "511"
  command = "/etc/tedge/operations/command"
```

And now the content of our command plugin:

```shell
#!/usr/bin/sh

mosquitto_pub -t c8y/s/us -m 501,c8y_Command

OUTPUT=$(echo $1)

mosquitto_pub -t c8y/s/us -m 503,c8y_Command,"$OUTPUT"
```

This simple example will execute the command `echo $1` and send the result back to the cloud.

> Note: The command will be executed with tedge-mapper permission level so most of the system level commands will not work.


### List of currently supported operations parameters

* `topic` - The topic on which the operation will be executed.
* `on_message` - The SmartRest template on which the operation will be executed.
* `command` - The command to execute.
