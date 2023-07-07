---
title: Mapper Configuration
tags: [Operate, Configuration, Cloud, MQTT]
---

# How to control which MQTT topics the mappers subscribe to

The cloud-specific mappers subscribe to the reserved MQTT topics and convert incoming MQTT messages to cloud-specific messages.
In an advanced use case, such as using more than one cloud mappers for the same device,
you may want to customize the external tedge MQTT topics that each cloud mapper subscribes to.

The `tedge config` command and the keys `c8y.topics`, `az.topics`, and `aws.topics` are usable for this use-case.

| Cloud          | tedge config key | Environmental variable | systemctl service |
|----------------|------------------|------------------------|-------------------|
| Cumulocity IoT | c8y.topics       | TEDGE_C8Y_TOPICS       | tedge-mapper-c8y  |
| Azure IoT      | az.topics        | TEDGE_AZ_TOPICS        | tedge-mapper-az   |
| AWS IoT        | aws.topics       | TEDGE_AWS_TOPICS       | tedge-mapper-aws  |

:::note
This guide uses `c8y.topics`, `TEDGE_C8Y_TOPICS`, and `tedge-mapper-c8y` as an example.
For other cloud mappers, use the keys in the table.
:::

## Check the subscribed MQTT topics

First, check which MQTT topics are subscribed by a cloud mapper. Run:

```sh
tedge config get c8y.topics
```

```sh title="Output"
["tedge/measurements", "tedge/measurements/+", "tedge/alarms/+/+", "tedge/alarms/+/+/+", "tedge/events/+", "tedge/events/+/+", "tedge/health/+", "tedge/health/+/+"]
```

## Set the desired new MQTT topics

If you want to change the subscribed MQTT topics, use `tedge config set`.
For example, if you want the Cumulocity IoT mapper to subscribe only `tedge/measurements` and `tedge/measurements/+` topics,
the command to run should be as below.

```sh
sudo tedge config set c8y.topics tedge/measurements,tedge/measurements/+
```

Alternatively, the same setting can be controlled via environment variables.
The environment variable settings will override any values set by the tedge config command.

```sh
export TEDGE_C8Y_TOPICS=tedge/measurements,tedge/measurements/+
```

:::note
If an invalid MQTT topic is given, the mapper will ignore it.
:::

The service must be restarted for the setting to take effect.
The following command shows how to restart the Cumulocity IoT mapper on a device using systemd as the init system.

```sh
sudo systemctl restart tedge-mapper-c8y
```

## Change back to the default topics

If you want a mapper to subscribe back to the default MQTT topics, run:

```sh
sudo tedge config unset c8y.topics
```

Then restart the corresponding mapper.
