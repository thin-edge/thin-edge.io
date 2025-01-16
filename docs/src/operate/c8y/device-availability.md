---
title: Availability Monitoring
tags: [Operate, Cumulocity, Monitoring]
description: Monitoring the availability of devices
---

# Availability Monitoring

%%te%% fully supports the [Cumulocity's device availability monitoring feature](https://cumulocity.com/docs/device-management-application/monitoring-and-controlling-devices/#availability) 
allowing you to set the desired required interval for the devices
and also sending heartbeats to **Cumulocity** periodically when a device is deemed available.
%%te%% considers a device as available when the `tedge-agent` service on it is up and running,
monitored using its service health endpoint.
The health endpoint can be changed from the `tedge-agent` to any other entity's health endpoint as well.

## Set the required availability interval

As described in the [Cumulocity's user documentation](https://cumulocity.com/docs/device-integration/fragment-library/#device-availability),
%%te%% main and child devices set their required interval during their first connection.
Availability monitoring is enabled by default with a default required interval of 1 hour.
The value to be updated using the `tedge config set` command as follows:

```sh
sudo tedge config set c8y.availability.interval 30m
```

If the value is set to less than 1 minute or 0,
availability monitoring is disabled and the device is considered to be in maintenance mode.

## Change the health endpoint for heartbeat messages

Once the device connection to **Cumulocity** is established,
**tedge-mapper-c8y** will keep sending heartbeat messages (empty inventory update messages) on behalf of the main and child devices
to keep their availability active.
By default, the status of the **tedge-agent** service is used to determine whether the device is available or not.

For example, the device `device/my-device//` is considered "available" when its tedge-agent service status is reported as `up` as shown below:

```sh te2mqtt formats=v1
tedge mqtt pub te/device/my-device/service/tedge-agent/status/health '{"status":"up"}' -q 2 -r
```

To change the health endpoint from the default to a custom value, include the `@health` property in the entity registration message.
The `@health` value should be a valid [entity topic identifier](../../contribute/design/mqtt-topic-design.md).

```sh te2mqtt formats=v1
tedge mqtt pub te/device/my-device// '{"@health":"device/my-device/service/foo", "@type":"child-device"}' -q 2 -r
```

If the status of the new endpoint is reported as `up`, the device is considered "available",
and a heartbeat signal is sent to Cumulocity.
If the status has any other value, the device is considered "unavailable",
and no heartbeat message are sent to Cumulocity until the status changes to `up` again.

## Disable the availability monitoring

By default, the feature is enabled.
To disable the feature, use the `tedge config set` command as follows and restart the **tedge-mapper-c8y** service.

```sh
sudo tedge config set c8y.availability.enable false
```

When disabled, the required availability interval and periodic heartbeat messages aren't sent to Cumulocity.
