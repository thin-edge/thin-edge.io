# How to connect?

## Connect to Cumulocity IoT​

To create northbound connection a local bridge shall be established and this can be achieved with `tedge` cli and following commands:
> Note: Some of the commands require elevated permissions to enable system services e.g. [`tedge connect`](../references/tedge-connect.md) needs to enable `mosquitto` server.

___

Configure required parameters for thin-edge.io with [`tedge config set`](../references/tedge-config.md):

```shell
tedge config set c8y.url example.cumulocity.com​
```

> Tip: If you you are unsure which parameters are required for the command to work run the command and it will tell you which parameters are missing.
> For example, if we issue [`tedge connect c8y`](../references/tedge-connect.md) without any configuration following advice will be given:
>
> ```shell
> $ tedge connect c8y`
> ...
> Error: failed to execute `tedge connect`.
>
> Caused by:
>     Required configuration item is not provided 'c8y.url', run 'tedge config set c8y.url <value>' to add it to config.
> ```
>
> This message explains which configuration parameter is missing and how to add it to configuration, in this case we are told to run `tedge config set c8y.url <value>`.

___

Next step is to upload self-signed certificate, which is not needed in production with root cert!​
You can upload root certificate via [Cumulocity UI](https://cumulocity.com/guides/10.7.0-beta/device-sdk/mqtt/#device-certificates) or with [`tedge cert upload`](../references/tedge-cert.md) as described below.

> Note: This command takes parameter `user`, this is due to upload mechanism to Cumulocity cloud which uses username and password for authentication.
>
> After issuing this command you are going to be prompted for a password. Users usernames and passwords are not stored in configuration due to security.

```shell
$ tedge cert upload c8y –-user <username>
Password:
```

where:
> `username` -> user in Cumulocity with permissions to upload new certificates

___

To create bridge use [`tedge connect`](../references/tedge-connect.md):

> Note: This command requires elevated permission.

```shell
$ tedge connect c8y
Checking if systemd and mosquitto are available.

Checking if configuration for requested bridge already exists.

Creating configuration for requested bridge.

Saving configuration for requested bridge.

Restarting mosquitto, [requires elevated permission], authorise when asked.

[sudo] password for user:
Awaiting mosquitto to start. This may take up to 5 seconds.

Sending packets to check connection.
Registering the device in Cumulocity if the device is not yet registered.
This may take up to 10 seconds per try.

Try 1 / 2: Sending a message to Cumulocity. ... Received message.
The device is connected to Cumulocity.

Persisting mosquitto on reboot.

Saving configuration.
Successfully created bridge connection!
```

### Errors

#### Connection already established

If connection has already been established following error may appear:

```shell
$ tedge connect c8y
Checking if systemd and mosquitto are available.

Checking if configuration for requested bridge already exists.

Error: failed to execute `tedge connect`.

Caused by:
    Connection is already established. To remove existing connection use 'tedge disconnect c8y' and try again.
```

To remove existing connection and create new one follow the advice and issue [`tedge disconnect c8y`](../references/tedge-disconnect.md):

```shell
$ tedge disconnect c8y
Removing c8y bridge.

Applying changes to mosquitto.

Bridge successfully disconnected!
```

And now you can issue [`tedge connect c8y`](../references/tedge-connect.md) to create new bridge.

#### Connection check failure

Sample output of tedge connect when this error occurs:

```shell
$ tedge connect c8y
Checking if systemd and mosquitto are available.

Checking if configuration for requested bridge already exists.

Creating configuration for requested bridge.

Saving configuration for requested bridge.

Restarting mosquitto, [requires elevated permission], authorise when asked.

[sudo] password for user:
Awaiting mosquitto to start. This may take up to 5 seconds.

Sending packets to check connection.
Registering the device in Cumulocity if the device is not yet registered.
This may take up to 10 seconds per try.

Try 1 / 2: Sending a message to Cumulocity. ... No response. If the device is new, its normal to get no response in the first try.
Try 2 / 2: Sending a message to Cumulocity. ... No response.
Warning: Bridge has been configured, but Cumulocity connection check failed.

Persisting mosquitto on reboot.

Saving configuration.
Successfully created bridge connection!
```

This error may be caused by some of the following reasons:

- No access to Internet connection

Local bridge has been configured and is running but the connection check has failed due to no access to the northbound endpoint.

- Cumulocity tenant not available

Tenant couldn't be reached and therefore connection check has failed.

- Check bridge

Bridge configuration is correct but the connection couldn't be established to unknown reason.

To retry start with [`tedge disconnect c8y`](../references/tedge-disconnect.md) removing this bridge:

```shell
tedge disconnect c8y
```

When this is done, issue [`tedge connect c8y`](../references/tedge-connect.md) again.

#### File permissions

Sample output:

```shell
$ tedge connect c8y
Checking if systemd and mosquitto are available.

Checking if configuration for requested bridge already exists.

Creating configuration for requested bridge.

Saving configuration for requested bridge.

Error: failed to execute `tedge connect`.

Caused by:
    0: File Error. Check permissions for /home/makrist/.tedge/bridges/c8y-bridge.conf.
    1: failed to persist temporary file: Permission denied (os error 13)
```

tedge connect cannot access location to create the bridge configuration (`/home/user/.tedge/bridges`), check permissions for the directory and adjust it to allow the tedge connect to access it.

Example of incorrect permissions:

```shell
$ ls -l
total 32
dr--r--r-- 2 user user 4096 Dec  31 11:40 bridges
```

If this comes up please use provided script to fix permissions: fix_permissions.sh

#### mosquitto and systemd check fails

Sample output:

```shell
$ tedge connect c8y
Checking if systemd and mosquitto are available.

Error: failed to execute `tedge connect`.

Caused by:
    mosquitto is not installed on the system. Install mosquitto to use this command.
```

mosquitto server has not been installed on the system and it is required to run this command, refer to [How to install thin-edge.io?](./002_installation.md) to install mosquitto and try again.

## Next steps

1. [How to use mqtt pub/sub?](./005_pub_sub.md)
