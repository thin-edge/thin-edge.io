---
title: MQTT topics
tags: [MQTT, Legacy]
sidebar_position: 1
description: Legacy %%te%% MQTT topics and how to map to the new topics
---

The most visible breaking change introduced by %%te%% 1.0 is the [new topic structure](../references/mqtt-api.md),
which has been made more consistent and extensible with a better support for child devices and services.

Porting an extension that publishes telemetry data on the legacy `tedge` topic, should not pose any difficulty:
- *measurements* and *events* can be mapped directly to the new scheme by just changing the topics
- for *alarms*, the severity in the topic in the old scheme must be mapped to the payload in the new scheme
- handling of child devices is now consistent for *measurements*, *events* and *alarms*.

## Backward compatibility

The **tedge-agent** running on the main device implements a compatibility layer
and republishes on the new topics any message received on the legacy topics.

Thanks to this compatibility mechanism a legacy extension works out of the box with %%te%% 1.0.

However, this mechanism will be deprecated medium-term, and we encourage you to port any legacy extension to the new API. 

## Telemetry: Main device

Here is the mapping between the legacy and new topics for the main device: 

<table style={{width:'100%'}}>
<tr>
  <th>Type</th>
  <th>Topic</th>
  <th>Payload Changes</th>
</tr>

<!-- Measurements -->
<tr>
  <td>Measurements</td>
  <td>
    <p>Legacy</p>

```sh
tedge/measurements
```

  <p>New</p>

```sh
te/device/main///m/<type>
```

  </td>
  <td>
    No Change. If "&lt;type&gt;" is not provided, a default value of "ThinEdgeMeasurement" will be used.
  </td>
</tr>

<!-- Events -->
<tr>
  <td>Events</td>
  <td>
    <p>Legacy</p>

```sh
tedge/events/<type>
```

  <p>New</p>

```sh
te/device/main///e/<type>
```

  </td>
  <td>
    No Change
  </td>
</tr>

<!-- Alarms -->
<tr>
  <td>Alarms</td>
  <td>
    <p>Legacy</p>

```sh
tedge/alarms/<severity>/<type>
```

  <p>New</p>

```sh
te/device/main///a/<type>
```

  </td>
  <td>

The alarm severity should be set in the payload.

```json5
{
  "severity": "<severity>"
  // ...
}
```

  </td>
</tr>

<!-- Health status -->
<tr>
  <td>Health status</td>
  <td>
    <p>Legacy</p>

```sh
tedge/health/<service_name>
```

  <p>New</p>

```sh
te/<service_topic_id>/status/health
```

  </td>
  <td>
    <code>type</code> property removed from the payload.
  </td>
</tr>

</table>


## Telemetry: Child device

Here is the mapping between the legacy and new topics for a child device:

<table style={{width:'100%'}}>
<tr>
  <th>Type</th>
  <th>Topic</th>
  <th>Payload Changes</th>
</tr>

<!-- Measurements -->
<tr>
  <td>Measurements</td>
  <td>
    <p>Legacy</p>

```sh
tedge/measurements/<child_id>
```

  <p>New</p>

```sh
te/device/<child_id>///m/<type>
```

  </td>
  <td>
    No Change. If the "&lt;type&gt;" is not provided, a default value of "ThinEdgeMeasurement" will be used.
  </td>
</tr>

<!-- Events -->
<tr>
  <td>Events</td>
  <td>
    <p>Legacy</p>

```sh
tedge/events/<type>/<child_id>
```

  <p>New</p>

```sh
te/device/<child_id>///e/<type>
```

  </td>
  <td>
    No Change
  </td>
</tr>

<!-- Alarms -->
<tr>
  <td>Alarms</td>
  <td>
    <p>Legacy</p>

```sh
tedge/alarms/<severity>/<type>/<child_id>
```

  <p>New</p>

```sh
te/device/<child_id>///a/<type>
```

  </td>
  <td>

The alarm severity should be set in the payload.

```json5
{
  "severity": "<severity>"
  // ...
}
```

  </td>
</tr>

<!-- Health status -->
<tr>
  <td>Health status</td>
  <td>
    <p>Legacy</p>

```sh
tedge/health/<child_id>/<service_name>
```

  <p>New</p>

```sh
te/<service_topic_id>/status/health
```

  </td>
  <td>
    <code>type</code> property removed from the payload.
  </td>
</tr>

</table>