# Connect

## Connect to Cumulocity IoT​

To create northbound conenction a local bridge shall be established and this can be achieved with `tedge` cli and following command:
> NB: Some of the commands require elevated permissions to enable system services e.g. [`tedge connect`](../references/tedge-connect.md) needs to enable `mosquitto` server.

```shell
tedge config set c8y.url example.cumulocity.com​
```

Upload self-signed certificate, not needed in production with root cert!​

> This command takes parameter `user`, this is due to upload mechanism to Cumulocity cloud which uses username and password for authentication.
> After issuing this command you are going to be prompted for a password.

```shell
$ tedge cert register c8y –-user <username>
Password:
```

where:
> `username` -> user in Cumulocity with permissions to upload new certificates

Add happy commands output
Add known unhappy paths, permission issue, file exists ...

To create bridge use [`tedge connect`](../references/tedge-connect.md):

> This command requires elevated permission.

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

If conenction has already been established following error may appear:

```shell
$ tedge connect c8y
Checking if systemd and mosquitto are available.

Checking if configuration for requested bridge already exists.

Error: failed to execute `tedge connect`.

Caused by:
    Connection is already established. To remove existing connection use 'tedge disconnect c8y' and try again.
```

To remove existing conenction and create new one follow the advice and issue [`tedge disconnect c8y`](../references/tedge-disconnect.md):

```shell
$ tedge disconnect c8y
Removing c8y bridge.

Applying changes to mosquitto.

Bridge successfully disconnected!
```

And now you can issue `tedge connect c8y` to create new bridge.
Next steps:

1. [Testing with MQTT pub and sub](./005_pub_sub.md)
