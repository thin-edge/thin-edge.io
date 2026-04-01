---
title: Mosquitto Configuration
tags: [Operate, Configuration, MQTT]
description: Mosquitto specific configuration guide
---

## Disabling the default mosquitto listener configuration {#mosquitto-bind-enabled}

By default, `tedge connect` creates the file `/etc/tedge/mosquitto-conf/tedge-mosquitto.conf`,
which configures mosquitto listener settings (bind address and port) recommended for %%te%%.

If you manage your own mosquitto configuration — for example because you are running an external
mosquitto broker or have custom listener settings — you can tell %%te%% **not** to create that file:

```sh
sudo tedge config set mqtt.bind.enabled false
```

With this setting, `tedge connect` will skip writing `tedge-mosquitto.conf` entirely,
leaving your existing mosquitto configuration untouched.
You are then responsible for ensuring that mosquitto is configured to match the
`mqtt.bind.address` and `mqtt.bind.port` values used by %%te%%.

## Configuring mosquitto bind address and port {#mosquitto-bind-address}

Configuring a mosquitto port and bind address in %%te%% is a three-step process.

:::note
The mqtt.bind.port and the mqtt.bind.address can be set/unset independently.
:::

### Step 1: Disconnect thin-edge.io edge device

The %%te%% device has to be disconnected from the cloud using the `tedge` command

```sh
tedge disconnect c8y

#or
tedge disconnect az

#or
tedge disconnect aws
```

### Step 2: Set the new mqtt port and bind address

Use the `tedge` command to set the mqtt.bind.port and mqtt.bind.address with a desired port and bind address as below.

```sh
sudo tedge config set mqtt.bind.port 1024
```

```sh
sudo tedge config set mqtt.bind.address 127.0.0.1
```

:::note
The bind_address is the address of one of the device network interface.
For example, this can be get as `ifconfig | grep inet` or set it to `0.0.0.0`
:::

This will make sure that all the mqtt clients use the newer port and the bind address that
has been set once the device is connected to the cloud as in step 3.

### Step 3: Connect the device to cloud

Use the `tedge` command to connect to the desired cloud as below.

```sh
tedge connect c8y

#or
tedge connect az

#or
tedge connect aws
```

This will configure all the services (mosquitto, tedge-mapper-c8y.service, tedge-mapper-az.service,
tedge-mapper-aws.service, tedge-agent.service) to use the newly set port and the bind address.

## Common Errors

The below example shows that we cannot set a string value for the port number.

```sh
tedge config set mqtt.bind.port '"1234"'
```

```text title="Output"
Error: failed to set the configuration key: mqtt.bind.port with value: "1234".

Caused by:
    Conversion from String failed
```

## Updating the mqtt port and bind address (host) in collectd and for collectd-mapper

Update the `collectd.conf` with the new port and host in `<Plugin mqtt>`.

Then, restart the collectd service.

```sh
sudo systemctl restart collectd
```

After changing the mqtt port and host, then connect to the cloud using `tedge connect c8y/az`.
Then (Steps 1-3) the collectd-mapper has to be restarted to use the newly set port and bind address (host).

Restart the tedge-mapper-collectd service.

```sh
sudo systemctl restart tedge-mapper-collectd
```
