---
title: Deregister entities
tags: [Child-Device, Registration]
sidebar_position: 2
description: Deregister child devices and services from %%te%%
---

# Deregister entities

All the entities (devices and services) registered with %%te%% are stored with the MQTT broker as retained messages,
with their metadata spread across multiple topics.
For example, for a child device `child01`, the registration message is stored on the `te/device/child01//` topic,
its twin data is stored across several `te/device/child01///twin/<twin-data-type>` topics,
its command metadata stored across several `te/device/child01///cmd/<cmd-type>` topics and so on.

On top of that, when a device has services associated with it, or has nested child devices,
they all have their own respective registration topics and other metadata topics.
So, deregistering a device involves deregistering itself, its metadata and
the complete entity hierarchy that is associated with it.

Even though %%te%% doesn't provide a direct API for this yet, this can be easily be done using third-party tools
like `mosquitto_sub` that supports clearing multiple retained messages together using the `--remove-retained` option.

## Deregister a child device and its services

When the topic ids of the devices and services follow the default topic scheme,
which maintains the service topic ids (`te/device/<device-id>/service/<service-id>`)
directly under the device topic id (`te/device/<device-id>//`) in the hierarchy,
a child device along with all of its services can be deleted as follows:

```sh
mosquitto_sub --remove-retained -W 3 -t "te/device/child01/+/+/#"
```

* The topic filter `te/device/child01/+/+` covers the registration messages of the device and all of its services,
  and the trailing `#` covers their their metadata messages as well.
* The `--remove-retained` clears all the retained messages matching that topic filter.
  Running the same command without the `remove-retained` option shows the list of messages
  that would be cleared with this command, which can be used for dry runs before the entities are really removed.
* The `-W 3` option is used to stop the command after a timeout period of 3 seconds, without which the command will not exit.
  Adjust this timeout value based on the number of entities in your deployment.

:::note
The entities in the cloud must also be removed separately as deregistrations on the device
are not propagated to the cloud.
The Cumulocity mapper must also be restarted if the same entity is to be recreated after deregistration.
:::

A single service of a device (main or child), can be deregistered as follows:

```sh
mosquitto_sub --remove-retained -W 3 -t "te/device/main/service/service01/#"
```

All the services associated to a device can be deregistered together, as follows:

```sh
mosquitto_sub --remove-retained -W 3 -t "te/device/child01/service/+/#"
```

:::note
De-registering parent and child entities together with a single command only works
when the parent and children are hierarchically linked via the topics, which can be queried with a single topic filter,
as in the case of services while following the default topic scheme.
While using custom topic schemes, that do not maintain the same topic prefix for a device and its services,
querying and deregistering them may not be a on-liner as in the examples listed above.
:::

## Deregister all child devices and services

All the child devices and services can be deregistered using a wildcard topic filter that covers all the entities
combined with a topic exclusion filter for the main device, as follows:

```sh
mosquitto_sub -v -t "te/device/+/#" -T "te/device/main/#" --remove-retained -W 3
```

## Unsupported cases

While using the default topic scheme, the following use-cases are not supported as it's not possible to define 
wildcard topic filters for the same, as the topic scheme does not capture the parent-child relationship of devices:

* Deregister all child devices of a given device
* Deregister the entire nested child device hierarchy of a device

## Custom topic schemes

Most of the commands listed above are applicable only when the default topic scheme is used.
The same rules, especially the ones involving multiple entities, do not apply to custom topic schemes,
as different entities would be linked differently based on the topic scheme that's used.
The filtering capabilities would also very from scheme to scheme.
As long as you can define topic filters to select individual entities or a set of entities,
the same can be used for deregistering as well.
