# thin-edge.io Supported Operations

## Supported Operations concepts

### Device operations

IoT devices often do more than just send data to the cloud. They also do things like:

* receive triggers from the operator
* reboot on demand
* install or remove software

These operations that are supported by [Cumulocity IoT](https://cumulocity.com/api/10.11.0/#section/Device-management-library) and other cloud providers.
On `thin-edge.io` the support for one such operation can be added using the `thin-edge.io` Supported Operations API.

### thin-edge.io Supported Operations API

The Supported Operations utilises the file system to add and remove operations. A special file placed in `/etc/tedge/operations` directory will indicate that an operation is supported.
The specification for the operation files is described in thin-edge.io specifications repository[src/supported-operations/README.md](https://github.com/thin-edge/thin-edge.io-specs/blob/a99a8cbf78a4c4c9637fb1794797cb2fb468a0f4/src/supported-operations/README.md)

## thin-edge.io List of Supported Operations

thin-edge.io supports natively the following operations:

* Software Update
* Software Update Log Upload
* Restart

The list is growing as we support more operations, but is not exhaustive and we encourage you to contribute to the list.

## How to use Supported Operations

### Listing current operations

You can obtain the current list of supported operations by listing the content of the `/etc/tedge/operations` directory.
This directory will contain a set subdirectories based on cloud providers currently supported eg:

```shell
$ ls -l /etc/tedge/operations

drwxr-xr-x 2 tedge tedge 4096 Jan 01 00:00 az
drwxr-xr-x 2 tedge tedge 4096 Jan 01 00:00 c8y
```

From the above you can see that there are two cloud providers supported by thin-edge.io.
The directories should be readable by thin-edge.io user - `tedge` - and should have permissions `755`.

To list all currently supported operations for a cloud provider, run:

```shell
$ ls -l /etc/tedge/operations

-rw-r--r-- 1 tedge tedge 0 Jan 01 00:00 c8y_Restart
```

To list all currently supported operations, run:

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
This operation will do the reboot of our device when we receive trigger from the operator. thin-edge.io device will receive an MQTT message with certain payload and we already have a handler for that payload in the `c8y-mapper`.

To add new operation we will create a file in `/etc/tedge/operations/c8y` directory:

```shell
sudo -u tedge touch /etc/tedge/operations/c8y/c8y_Restart
```

> Note: We are using `sudo -u` to create the file because we want to make sure that the file is owned by `tedge` user.

Now we just need to reboot the `c8y-mapper` so it picks new operations and it will automatically add it to the list and send it to the cloud.

### Removing supported operations

To remove supported operation we can remove the file from `/etc/tedge/operations/c8y` directory and restart the `c8y-mapper` to pick up the new list of supported operations. eg:

```shell
sudo rm /etc/tedge/operations/c8y/c8y_Restart
```
