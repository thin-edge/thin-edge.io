# How to configure the local port and bind address in mosquitto?

Configuring a mosquitto port and bind address in thin-edge.io is a three-step process.

> Note: The mqtt.port and the mqtt.bind_address can be set/unset independently.

## Step 1: Disconnect thin-edge.io edge device

The thin edge device has to be disconnected from the cloud using the `tedge` command

```shell
tedge disconnect c8y/az
```

## Step 2: Set and verify the new mqtt port and bind address

Use the `tedge` command to set the mqtt.port and mqtt.bind_address with a desired port and bind address as below.

```shell
tedge config set mqtt.port 1024
```

```shell
tedge config set mqtt.bind_address 127.0.0.1
```

> Note: The bind_address is the address of the one of the device interface.
  For example this can be get as `ifconfig | grep inet` or set it to `0.0.0.0`

This will make sure that all the mqtt clients use the newer port and the bind address that
has been set once the device is connected to the cloud as in step 3.

## Verify the port and the bind address configured/set

Use the `tedge` command to print the mqtt port and bind address that has been set as below.

```shell
tedge config get mqtt.port
```

```shell
tedge config get mqtt.bind_address
```

## Step 3: Connect the thin edge device to cloud

Use the `tedge` command to connect to the desired cloud as below.

```shell
tedge connect c8y

#or

tedge connect az

#or

tedge connect aws
```

This will configure all the services (mosquitto, tedge-mapper-c8y.service, tedge-mapper-az.service,
  tedge-mapper-aws.service, tedge-agent.service) to use the newly set port and the bind address.
  
Note: The step 1 and 2 can be followed in any order.

## Update to use default port and bind address

Use the `tedge` command to set the default port (1883) and default bind address (localhost) as below.

```shell
tedge config unset mqtt.port
```

```shell
tedge config unset mqtt.bind_address
```

Once the port or the bind address is reverted to default, the [step 1](#Step-3:-Connect-the-thin-edge-device-to-cloud)
and 3 has to be followed to use the default port or the default bind address.

## Error case

The below example shows that we cannot set a string value for the port number.

```shell
tedge config set mqtt.port '"1234"'

Error: failed to set the configuration key: mqtt.port with value: "1234".

Caused by:
    Conversion from String failed
```

## Updating the mqtt port and bind address (host) in collectd & for collectd-mapper

Update the `collectd.conf` with the new port and host in `<Plugin mqtt>`.

Then, restart the collectd service.

```shell
sudo systemctl restart collectd.service
```

After changing the mqtt port and host, then connect to the cloud using `tedge connect c8y/az`.
Then (Steps 1-3) the collectd-mapper has to be restarted to use the newly set port and bind address (host).

Restart the tedge-mapper-collectd service.

```shell
sudo systemctl restart tedge-mapper-collectd.service
```
