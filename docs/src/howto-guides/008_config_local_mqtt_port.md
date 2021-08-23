# How to configure the local port in mosquitto?

Configuring a mosquitto port in thin edge is a three step process.

## Step 1: Disconnect the thin edge device

The thin edge device has to be disconnected from the cloud using the `tedge` command

```shell
tedge disconnect c8y/az
```  

## Step 2: Set and verify the new mqtt port 

Set the mqtt.port with a desired port using `tedge` command as below.
   
```shell
tedge config set mqtt.port 1024
```
This will make sure that all the mqtt clients use the newer port that has been set.

## Verify the port configured/set

Use below command to check the port has been set properly or not.
 
```shell
tedge config get mqtt.port
```
This prints out the port that has been set.

## Step 3: Connect the thin edge device to cloud

Using `tedge` command connect to the desired cloud as below.

This will force all the services (mosquitto, tedge-mapper-c8y.service,tedge-mapper-az.service sm_agent,
tedge-agent.service, tedge-mapper-sm-c8y.service) to use newely set port.

```shell
tedge connect c8y

#or

tedge connect az
```


Note: The step 1 and 2 can be followed in any order.

## Update to use default port

To use the default port (1883), the mqtt.port has to be unset using the `tedge` command as below

```shell
tedge config unset mqtt.port
```
Once the port is reverted to default, the [step 1](#Step-3:-Connect-the-thin-edge-device-to-cloud)
and 3 has to be followed to use the default port.

## Error case

Below example shows that we can not set string value for the port number.

```shell
tedge config set mqtt.port '"1234"'

Error: failed to set the configuration key: mqtt.port with value: "1234".

Caused by:
    Conversion from String failed
```

## Updating the mqtt port in collectd & for collectd-mapper

Update the `collectd.conf` with the new port in `<Plugin mqtt>`

Restart the collectd service

```shell
sudo systemctl restart collectd.service
```

After changing the mqtt port and connected to cloud using `tedge connect c8y/az`,
(Steps 1-3) the collectd-mapper has to be restarted to use the newly set port.

Restart the collectd-mapper service

```shell
sudo systemctl restart collectd-mapper.service
```
