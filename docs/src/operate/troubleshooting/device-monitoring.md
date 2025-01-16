---
title: Device Monitoring
tags: [Operate, Monitoring]
sidebar_position: 1
description: How to troubleshoot device monitoring
---

To install and configure monitoring on your device,
see the tutorial [Monitor your device with collectd](../../start/device-monitoring.md).

## Is collectd running?

```sh
sudo systemctl status collectd
```

If not, launch collected

```sh
sudo systemctl start collectd
```

## Is collectd publishing MQTT messages?

```sh te2mqtt formats=v1
tedge mqtt sub 'collectd/#'
```

If no metrics are collected, please check the [MQTT configuration](../../start/device-monitoring.md#collectd-configuration)

:::note
The `collectd.conf` file included with %%te%% is configured for conservative interval times, e.g. 10 mins to 1 hour depending on the metric. This is done so that the metrics don't consume unnecessary IoT resources both on the device and in the cloud. If you want to push the metrics more frequently then you will have to adjust the `Interval` settings either globally or on the individual plugins. Make sure you restart the collectd service after making any changes to the configuration.
:::

## Is the tedge-mapper-collectd running?

```sh
sudo systemctl status tedge-mapper-collectd
```

If not, launch tedge-mapper-collectd.service as below

```sh
sudo systemctl start tedge-mapper-collectd
```

## Are the collectd metrics published in Thin Edge JSON format?

```sh te2mqtt formats=v1
tedge mqtt sub 'te/device/main///m/+'
```

## Are the collectd metrics published to Cumulocity?

```sh te2mqtt formats=v1
tedge mqtt sub 'c8y/#'
```

If not see how to [connect a device to Cumulocity](../../start/connect-c8y.md).

## Are the collectd metrics published to Azure IoT?

```sh te2mqtt formats=v1
tedge mqtt sub 'az/#'
```

If not see how to [connect a device to Azure IoT](../../start/connect-azure.md).
