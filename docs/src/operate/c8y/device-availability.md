---
title: Availability Monitoring
tags: [Operate, Cumulocity, Monitoring]
description: Monitoring the availability of devices
---

# Availability Monitoring

%%te%% fully supports the [Cumulocity IoT's device availability monitoring feature](https://cumulocity.com/docs/device-management-application/monitoring-and-controlling-devices/#availability) by setting the required interval for devices and sending heartbeats to **Cumulotity IoT** periodically.


## Set the required availability interval

As described in the [Cumulocity IoT's user documentation](https://cumulocity.com/docs/device-integration/fragment-library/#device-availability),
%%te%% main and child devices set their required interval during their first connection.
The value to be set can be configured using the `tedge` command.

```sh
sudo tedge config set c8y.availability.interval 30m
```

If the value is set to less than 1 minute or 0, the device is considered to be in maintenance mode.

## Change the health endpoint for heartbeat messages

Once the device connection to **Cumulocity IoT** is established, **tedge-mapper-c8y** will keep sending empty inventory update messages on behalf of the main and child devices.
By default, the status of the **tedge-agent** service is used to determine whether the device is available or not.

For example, the device `device/my-device//` is considered "available" when its tedge-agent service status is reported as `up` as shown below:
```sh te2mqtt formats=v1
tedge mqtt pub te/device/my-device/service/tedge-agent/status/health '{"status":"up"}' -q 2 -r
```

To change the health endpoint from the default to a custom value, include `@health` in the entity registration message.
`@health` should have a valid [4-segment identifier](../../contribute/design/mqtt-topic-design.md).

```sh te2mqtt formats=v1
tedge mqtt pub te/device/my-device// '{"@health":"device/my-device/service/foo", "@type":"child-device"}' -q 2 -r
```

Then, if the status of the new endpoint is reported as `up`, the device is considered "available".
If the status has other values than `up`, the device is considered "unavailable",
and no heartbeat message will be sent to the tenant unless the status changes to `up`.
```sh te2mqtt formats=v1
tedge mqtt pub te/device/my-device/service/foo/status/health '{"status":"up"}' -q 2 -r
```

## Disable the availability monitoring

By default, the feature is enabled. If you want to disable it, use the `tedge` command and restart the **tedge-mapper-c8y** service.

```sh
sudo tedge config set c8y.availability.enable false
```

If it's disabled, the required availability interval and periodic heartbeat messages won't be sent to your Cumulocity IoT tenant.
