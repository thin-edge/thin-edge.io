# How to trouble shoot device monitoring

To install and configure monitoring on your device,
see the tutorial [Monitor your device with collectd](../tutorials/device-monitoring.md).

## Is collectd running?

```
sudo systemctl status collectd
```

If not, launch collected

```
sudo systemctl start collectd
```

## Is collectd publishing MQTT messages?

```
tedge mqtt sub 'collectd/#'
```

If no metrics are collected, please check the [MQTT configuration](../tutorials/device-monitoring.md#collectdconf)

## Is the thin-edge `collectd-mapper` running?

```
sudo systemctl status collectd-mapper
```

If not, launch collected

```
sudo systemctl start collectd-mapper
```

## Are the collectd metrics published in Thin Edge JSON format?

```
tedge mqtt sub 'tedge/measurements'
```

## Are the collectd metrics published to Cumulocity IoT?

```
tedge mqtt sub 'c8y/#'
```

If not see how to [connect a device to Cumulocity IoT](../tutorials/connect-c8y.md)

## Are the collectd metrics published to Azure IoT?

```
tedge mqtt sub 'az/#'
```

If not see how to [connect a device to Azure IoT](../tutorials/connect-azure.md)
