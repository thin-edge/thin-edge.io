---
title: Troubleshooting Device Monitoring
tags: [Operate, Monitoring]
sidebar_position: 1
---

# How to trouble shoot device monitoring

To install and configure monitoring on your device,
see the tutorial [Monitor your device with collectd](../../start/device-monitoring.md).

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

If no metrics are collected, please check the [MQTT configuration](../../start/device-monitoring.md#collectdconf)

## Is the `tedge-mapper-collectd.service` running?

```
sudo systemctl status tedge-mapper-collectd.service
```

If not, launch tedge-mapper-collectd.service as below

```
sudo systemctl start tedge-mapper-collectd.service
```

## Are the collectd metrics published in Thin Edge JSON format?

```
tedge mqtt sub 'tedge/measurements'
```

## Are the collectd metrics published to Cumulocity IoT?

```
tedge mqtt sub 'c8y/#'
```

If not see how to [connect a device to Cumulocity IoT](../../start/connect-c8y.md)

## Are the collectd metrics published to Azure IoT?

```
tedge mqtt sub 'az/#'
```

If not see how to [connect a device to Azure IoT](../../start/connect-azure.md)
