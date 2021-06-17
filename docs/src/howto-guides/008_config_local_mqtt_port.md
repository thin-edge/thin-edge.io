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

This will force all the services use newer port.

```shell
tedge connect c8y/az
```

Note: The step 1 and 2 can be followed any order.

## Update to use default port

To use the default port (1883), the mqtt.port has to be unset using the `tedge` command as below

```shell
tedge config unset mqtt.port
```
Also should follow step 1 and 3.

## Error case

Below example shows that we can not set string value for the port number.

```shell
tedge config set mqtt.port '"1024"'

Error: failed to set the configuration key: mqtt.port with value: "1024".

Caused by:
    Conversion from String failed
```

## Updating the mqtt port in collectd

Update the `collectd.conf` with the newer port in `<Plugin mqtt>`

Restart the collectd

```shell
sudo systemctl restart collectd.service
```

