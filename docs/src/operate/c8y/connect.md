---
title: Cloud Connection
tags: [Operate, Cloud]
sidebar_position: 1
---

# How to connect?

## Connect to Cumulocity IoT

To create northbound connection a local bridge shall be established and this can be achieved with `tedge` cli and following commands:

:::note
`tedge connect` requires `sudo` privilege.
:::

## Setting the cloud end-point

Configure required parameters for thin-edge.io with [`tedge config set`](../../references/cli/tedge-config.md):

```sh
sudo tedge config set c8y.url example.cumulocity.com
```

:::info
If you are unsure which parameters required by the command, simply run the command and it will tell you which parameters are missing.

For example, if we issue [`tedge connect c8y`](../../references/cli/tedge-connect.md) without any configuration following advice will be given:

```sh
sudo tedge connect c8y
```

```sh title="Output"
...
Error: failed to execute `tedge connect`.

Caused by:
    Required configuration item is not provided 'c8y.url', run 'tedge config set c8y.url <value>' to add it to config.
```

This message explains which configuration parameter is missing and how to add it to configuration,
in this case we are told to run `tedge config set c8y.url <value>`.
:::

## Making the cloud trust the device

The next step is to have the device certificate trusted by Cumulocity. This is done by uploading the certificate of the signee.
You can upload the root certificate using the [Cumulocity UI](https://cumulocity.com/guides/users-guide/device-management/#trusted-certificates)
or with [`tedge cert upload`](../../references/cli/tedge-cert.md) command as described below.

:::note
The `tedge cert upload` command requires the credentials of a Cumulocity user
having the permissions to upload trusted certificates on the Cumulocity tenant of the device.

The user name is provided as `--user <username>` parameter,
and the command will prompt you for this user's password.
These credentials are used only for this upload and will in no case be stored on the device.
:::

```sh
sudo tedge cert upload c8y --user "${C8Y_USER}"
```

```sh title="Example"
sudo tedge cert upload c8y --user "john.smith@example.com"
```

## Creating an MQTT bridge between the device and the cloud

The connection from the device to the cloud is established using a so-called MQTT bridge:
a permanent secured bidirectional MQTT connection that forward messages back and forth
between the two end-points.

To create the bridge use the [`tedge connect`](../../references/cli/tedge-connect.md) command.

```sh
sudo tedge connect c8y
```

```text title="Output"
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Creating the device in Cumulocity cloud.

Saving configuration for requested bridge.

Restarting mosquitto service.

Awaiting mosquitto to start. This may take up to 5 seconds.

Enabling mosquitto service on reboots.

Successfully created bridge connection!

Sending packets to check connection. This may take up to 2 seconds.

Connection check is successful.

Checking if tedge-mapper is installed.

Starting tedge-mapper-c8y service.

Persisting tedge-mapper-c8y on reboot.

tedge-mapper-c8y service successfully started and enabled!

Enabling software management.

Checking if tedge-agent is installed.

Starting tedge-agent service.

Persisting tedge-agent on reboot.

tedge-agent service successfully started and enabled!
```

## Errors

### Connection already established

If connection has already been established following error may appear:

```sh
sudo tedge connect c8y
```

```text title="Output"
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Error: failed to create bridge to connect Cumulocity cloud.

Caused by:
    Connection is already established. To remove existing connection use 'tedge disconnect c8y' and try again.
```

To remove existing connection and create new one follow the advice and issue [`tedge disconnect c8y`](../../references/cli/tedge-disconnect.md):

```sh
sudo tedge disconnect c8y
```

```text title="Output"
Removing Cumulocity bridge.

Applying changes to mosquitto.

Cumulocity Bridge successfully disconnected!

Stopping tedge-mapper-c8y service.

Disabling tedge-mapper-c8y service.

tedge-mapper-c8y service successfully stopped and disabled!

Stopping tedge-agent service.

Disabling tedge-agent service.

tedge-agent service successfully stopped and disabled!
```

:::note
`tedge disconnect c8y` also stops and disables the **tedge-mapper** service if it is installed on the device.
:::

And now you can issue [`tedge connect c8y`](../../references/cli/tedge-connect.md) to create new bridge.

### Connection check warning

Sample output of tedge connect when this warning occurs:

```sh
sudo tedge connect c8y
```

```text title="Output"
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Creating the device in Cumulocity cloud.

Saving configuration for requested bridge.

Restarting mosquitto service.

Awaiting mosquitto to start. This may take up to 5 seconds.

Enabling mosquitto service on reboots.

Successfully created bridge connection!

Sending packets to check connection. This may take up to 2 seconds.

ERROR: Local MQTT publish has timed out.
Warning: Bridge has been configured, but Cumulocity connection check failed.

Checking if tedge-mapper is installed.

Starting tedge-mapper-c8y service.

Persisting tedge-mapper-c8y on reboot.

tedge-mapper-c8y service successfully started and enabled!

Enabling software management.

Checking if tedge-agent is installed.

Starting tedge-agent service.

Persisting tedge-agent on reboot.

tedge-agent service successfully started and enabled!
```

This warning may be caused by some of the following reasons:

- No access to Internet connection

Local bridge has been configured and is running but the connection check has failed due to no access to the northbound endpoint.

- Cumulocity tenant not available

Tenant couldn't be reached and therefore connection check has failed.

- Check bridge

Bridge configuration is correct but the connection couldn't be established to unknown reason.

To retry start with [`tedge disconnect c8y`](../../references/cli/tedge-disconnect.md) removing this bridge:

```sh
sudo tedge disconnect c8y
```

When this is done, issue [`tedge connect c8y`](../../references/cli/tedge-connect.md) again.

### File permissions

Connecting without sudo will result in the following error:

```sh
tedge connect c8y
```

```text title="Output"
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Saving configuration for requested bridge.

Error: failed to create bridge to connect Cumulocity cloud.

Caused by:
    0: File Error. Check permissions for /etc/tedge/mosquitto-conf/tedge-mosquitto.conf.
    1: failed to persist temporary file: Permission denied (os error 13)
    2: Permission denied (os error 13)
```

tedge connect cannot access directory to create the bridge configuration (`/etc/tedge/mosquitto-conf`), check permissions for the directory and adjust it to allow the tedge connect to access it.

Example of incorrect permissions:

```sh
ls -l /etc/tedge
```

```text title="Output"
dr--r--r-- 2 tedge     tedge     4096 Mar 30 15:40 mosquitto-conf
```

You should give it the permission 755.

```sh
ls -l /etc/tedge
```

```text title="Output"
drwxr-xr-x 2 tedge     tedge     4096 Mar 30 15:40 mosquitto-conf
```

### Mosquitto and systemd check fails

Sample output:

```sh
sudo tedge connect c8y
```

```text title="Output"
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Saving configuration for requested bridge.

Restarting mosquitto service.

Error: failed to create bridge to connect Cumulocity cloud.

Caused by:
    Service mosquitto not found. Install mosquitto to use this command.
```

mosquitto server has not been installed on the system and it is required to run this command, refer to [How to install thin-edge.io?](../../install/index.md) to install mosquitto and try again.
