---
title: Cumulocity Mapper
tags: [Reference, Mappers, Cloud]
sidebar_position: 1
---

The Cumulocity mapper, referred to as `c8y-mapper` in the rest of this document,
maps data in [%%te%% format](../mqtt-api.md) into their equivalent [Cumulocity format](https://cumulocity.com/docs/smartrest/smartrest-two/).


## Registration

Cumulocity keeps the record of all the registered devices and their associated metadata in its inventory.
The `c8y-mapper` creates and maintains inventory entries for all the devices and services registered with %%te%%.
The mapper subscribes to the following topics to get a list of all registered devices and services:

```sh
mosquitto_sub -t 'te/+' -t 'te/+/+' -t 'te/+/+/+' -t 'te/+/+/+/+'
```

The registration messages received for child devices and services are mapped as follows:

### Child device

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/child01//
```

```json5 title="Payload"
{
  "@type": "child-device",
  "name": "child01",
  "type": "SmartHomeHub"
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
101,<main-device-id>:device:child01,child01,SmartHomeHub
```

</div>

Where the `<main-device-id>` is added as the prefix to the external id to avoid id clashes
with devices using the same name in other tedge deployments connected to the same tenant.

### Child device with explicit id

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/child01//
```

```json5 title="Payload"
{
  "@type": "child-device",
  "@id": "child01",
  "name": "child01",
  "type": "SmartHomeHub"
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
101,child01,child01,SmartHomeHub
```

</div>

Where the provided `@id` is directly used as the external id.

### Nested child device

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/nested_child01//
```

```json5 title="Payload"
{
  "@type": "child-device",
  "@parent": "device/child01//",
  "name": "nested_child01",
  "type": "BatterySensor"
}
```

</div>


<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us/<main-device-id>:device:child01
```

```text title="Payload"
101,<main-device-id>:device:nested_child01,nested_child01,BatterySensor
```

</div>

### Main device service

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/main/service/nodered
```

```json5 title="Payload"
{
  "@type": "service",
  "name": "Node-Red",
  "type": "systemd"
}
```

</div>


<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
102,<main-device-id>:device:main:service:nodered,systemd,Node-Red,up
```

</div>

### Child device service

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/child01/service/nodered
```

```json5 title="Payload"
{
  "@type": "service",
  "name": "Node-Red",
  "type": "systemd"
}
```

</div>


<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us/<main-device-id>:device:child01
```

```text title="Payload"
102,<main-device-id>:device:child01:service:nodered,systemd,Node-Red,up
```

</div>

Where the unique external IDs to be used in the cloud are derived from the entity identifier subtopics,
replacing the `/` characters with `:`.

:::note
The main device is registered with the cloud via the `tedge connect c8y` command execution
and hence there is no mapping done for main device registration messages.
Inventory data updates for the main device are handled differently.
:::

## Auto Registration of an entity

Before any data messages from an entity can be processed, the entity has to be registered first.
The entity can be registered either explicitly or implicitly (Auto registration).

With auto-registration, an entity does not need to explicitly send a registration message,
and the registration is done automatically by the mapper on receipt of the first message from that entity.
But, auto-registration is allowed only when the entity follows the default topic scheme: `te/device/<device-id>/service/<service-id>`.

For example, sending a measurement message to `te/device/child1///m/temperature` will result in the auto-registration of the device entity with topic id: `device/child1//` and the auto-generated external id: `<main-device-id>:device:child1`, derived from the topic id.
Similarly, a measurement message on `te/device/child1/service/my-service/m/temperature` results in the auto-registration of both
the device entity: `device/child1//` and the service entity: `device/child1/service/my-service` with their respective auto-generated external IDs, in that order.

**Pros:**
* No need to have a separate registration message for an entity.
   This would be ideal for simple devices where programming an additional registration logic is not possible ( e.g: simple sensors that can only generate telemetry messages).

**Cons:**
* Auto-registration of all entities is not possible in complex deployments with nested/hierarchical devices, as the parent of a nested child device can't be identified solely from its topic id (e.g: `te/device/nested-child//`).
The parent information must be provided explicitly using a registration message so that any nested child devices are not wrongly auto-registered as immediate child devices of the main device.
* Auto-registration results in the auto-generation of the device external id as well. If the user wants more control over it, then an explicit registration must be done.
	
Auto-registration can be enabled/disabled based on the complexity of the deployment.
For simpler deployments with just a single level child devices, following the default topic scheme,
auto-registration can be kept enabled.
For any complex deployments requiring external id customizations or with nested child devices,
auto-registration **must be disabled**.

Auto-registration can be disabled using the following `tedge config` command:
```sh
sudo tedge config set c8y.entity_store.auto_register false
```

Auto-registration is enabled, by default.
When the auto registration is disabled, and if the device is not explicitly registered,
then the c8y-mapper will ignore all the data messages received from that device,
logging that error message on the `te/errors` topic indicating that the entity is not registered.


## Telemetry

Telemetry data types like measurements, events and alarms are mapped to their respective equivalents in Cumulocity as follows:

### Measurement

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/main///m/environment
```

```json5 title="Payload"
{
  "temperature": 23.4
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/measurement/measurements/create
```

```json5 title="Payload"
{
  "type": "environment",
  "time": "2021-04-22T17:05:26.958340390+00:00",
  "temperature": {
    "temperature": {
      "value": 23
    }
  }
}
```

</div>


#### Measurement without type

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/main///m/
```

```json5 title="Payload"
{
  "temperature": 23.4
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/measurement/measurements/create
```

```json5 title="Payload"
{
  "type": "ThinEdgeMeasurement",
  "time": "2021-04-22T17:05:26.958340390+00:00",
  "temperature": {
    "temperature": {
      "value": 23
    }
  }
}
```

</div>


#### Measurement of child device

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/child01///m/
```

```json5 title="Payload"
{
  "temperature": 23.4
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/measurement/measurements/create
```

```json5 title="Payload"
{
  "externalSource":{
    "externalId":"<main-device-id>:device:child01",
    "type":"c8y_Serial"
  },
  "type":"ThinEdgeMeasurement",
  "time":"2013-06-22T17:03:14+02:00",
  "temperature":{
    "temperature":{
      "value":23
    }
  }
}
```

</div>

### Events

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/main///e/login_event
```

```json5 title="Payload"
{
  "text": "A user just logged in",
  "time": "2021-01-01T05:30:45+00:00"
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
400,login_event,"A user just logged in",2021-01-01T05:30:45+00:00
```

</div>

#### Event - Complex

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/main///e/login_event
```

```json5 title="Payload"
{
  "text": "A user just logged in",
  "time": "2021-01-01T05:30:45+00:00",
  "customFragment": {
    "nested": {
      "value": "extra info"
    }
  }
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/event/events/create
```

```json5 title="Payload"
{
  "externalSource":{
    "externalId":"<main-device-id>",
    "type":"c8y_Serial"
  },
  "type":"login_event",
  "text":"A user just logged in",
  "time":"2021-01-01T05:30:45+00:00",
  "customFragment": {
    "nested": {
      "value": "extra info"
    }
  }
}
```

</div>

### Alarms

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/main///a/temperature_high
```

```json5 title="Payload"
{
  "severity": "critical",
  "text": "Temperature is very high",
  "time": "2021-01-01T05:30:45+00:00"
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
301,temperature_high,"Temperature is very high",2021-01-01T05:30:45+00:00
```

</div>

#### Alarm - Complex

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/main///a/pressure_alarm
```

```json5 title="Payload"
{
  "severity": "major",
  "time": "2023-01-25T18:41:14.776170774Z",
  "text": "Pressure alarm",
  "customFragment": {
    "nested": {
      "value": "extra info"
    }
  }
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/alarm/alarms/create
```

```json5 title="Payload"
{
  "externalSource": {
    "externalId": "<main-device-id>",
    "type": "c8y_Serial"
  },
  "type": "pressure_alarm",
  "severity": "MAJOR",
  "time": "2023-01-25T18:41:14.776170774Z",
  "text": "Pressure alarm",
  "customFragment": {
    "nested": {
      "value": "extra info"
    }
  },
}
```

</div>

### Health status

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic"
te/device/main/service/my-service/status/health
```

```json5 title="Payload"
{
  "status": "up",
  "pid": 21037
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us/<service-external-id>
```

```text title="Payload"
104,up
```

</div>


## Twin

The `twin` metadata is mapped to [inventory data](https://cumulocity.com/docs/concepts/domain-model/#inventory) in Cumulocity.

#### Twin - Main device

A device's digital twin model can be updated by publishing to a specific topic.

The type part of the topic is used to group the data so it is easier for components to subscribe to relevant parts.

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic (retain=true)"
te/device/main///twin/device_OS
```

```json5 title="Payload"
{
  "family": "Debian",
  "version": "11"
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/inventory/managedObjects/update/<main-device-id>
```

```json5 title="Payload"
{
  "device_OS": {
    "family": "Debian",
    "version": "11"
  }
}
```

</div>


#### Twin - Child Device

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic (retain=true)"
te/device/child01///twin/device_OS
```

```json5 title="Payload"
{
  "family": "Debian",
  "version": "11"
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/inventory/managedObjects/update/<main-device-id>:device:child01
```

```json5 title="Payload"
{
  "device_OS": {
    "family": "Debian",
    "version": "11"
  }
}
```

</div>

#### Twin - Service on Main Device

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic (retain=true)"
te/device/main/service/tedge-agent/twin/runtime_stats
```

```json5 title="Payload"
{
  "memory": 3024,
  "uptime": 86400
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/inventory/managedObjects/update/<main-device-id>:device:main:service:tedge-agent
```

```json5 title="Payload"
{
  "runtime_stats": {
    "memory": 3.3,
    "uptime": 86400
  }
}
```

</div>


#### Twin - Service on Child Device

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic (retain=true)"
te/device/child01/service/tedge-agent/twin/runtime_stats
```

```json5 title="Payload"
{
  "memory": 3.3,
  "uptime": 86400
}
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/inventory/managedObjects/update/<main-device-id>:device:child01:service:tedge-agent
```

```json5 title="Payload"
{
  "runtime_stats": {
    "memory": 3.3,
    "uptime": 86400
  }
}
```

</div>


### Twin data - Root fragments

Data can be added on the root level of the twin by publishing the values directly to the topic with the key used as type.
The payload can be any valid JSON value other than a JSON object.
JSON objects must be published to their typed topics directly.

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic (retain=true)"
te/device/main///twin/subtype
```

```json5 title="Payload"
"my-custom-type"
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/inventory/managedObjects/update/<main-device-id>
```

```json5 title="Payload"
{
  "subtype": "my-custom-type"
}
```

</div>

:::warning
Updating the following properties via the `twin` channel is not supported

* `name`
* `type`

as they are included in the entity registration message and can only be updated with another registration message.
:::


### Twin - Deleting a fragment

<div class="code-indent-left">

**%%te%% (input)**

```text title="Topic (retain=true)"
te/device/child01/service/tedge-agent/twin/runtime_stats
```

```json5 title="Payload"
<<empty>>
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/inventory/managedObjects/update/<main-device-id>:device:child01:service:tedge-agent
```

```json5 title="Payload"
{
  "runtime_stats": null
}
```

</div>

### Base inventory model

The contents of `{tedge_config_dir}/device/inventory.json` are used to populate the initial inventory fragments
of the the main %%te%% device in Cumulocity.
For example, if the `inventory.json` contains the following fragments:

```json title="inventory.json"
{
  "c8y_Firmware": {
    "name": "raspberrypi-bootloader",
    "version": "1.20140107-1",
    "url": "31aab9856861b1a587e2094690c2f6e272712cb1"
  },
  "c8y_Hardware": {
    "model": "BCM2708",
    "revision": "000e",
    "serialNumber": "00000000e2f5ad4d"
  }
}
```

It is mapped to the following Cumulocity message:

```text title="Topic"
c8y/inventory/managedObjects/update
```

```json5 title="Payload"
{
  "c8y_Agent": {
    "name": "thin-edge.io",
    "url": "https://thin-edge.io",
    "version": "x.x.x"
  },
  "c8y_Firmware": {
    "name": "raspberrypi-bootloader",
    "version": "1.20140107-1",
    "url": "31aab9856861b1a587e2094690c2f6e272712cb1"
  },
  "c8y_Hardware": {
    "model": "BCM2708",
    "revision": "000e",
    "serialNumber": "00000000e2f5ad4d"
  }
}
```

Where the `c8y_Agent` fragment is auto-generated by %%te%% and appended to the contents of the file before it is published.

The fragments in this file are also published to the `te/device/main///twin/<fragment-key>` topics so that
the local twin metadata on the broker is also up-to-date and other components can also consume it.
For example, the above `inventory.json` would result in the following `twin` messages:

```text title="Topic"
te/device/main///twin/c8y_Agent
```

```json5 title="Payload"
{
  "name": "thin-edge.io",
  "url": "https://thin-edge.io",
  "version": "x.x.x"
}
```

```text title="Topic"
te/device/main///twin/c8y_Firmware
```

```json5 title="Payload"
{
  "name": "raspberrypi-bootloader",
  "version": "1.20140107-1",
  "url": "31aab9856861b1a587e2094690c2f6e272712cb1"
}
```

```text title="Topic"
te/device/main///twin/c8y_Hardware
```

```json5 title="Payload"
{
  "model": "BCM2708",
  "revision": "000e",
  "serialNumber": "00000000e2f5ad4d"
}
```

:::warning
The following keys in the `inventory.json` file are also ignored:

* `name`
* `type`

as they are included in the entity registration message and can only be updated with another registration message.
:::

### Updating entity type in inventory

After updating the inventory with `inventory.json` file contents, 
the device `type` of the main device, set using the `device.type` tedge config key,
is also updated in the inventory with the following message:

```text title="Topic"
c8y/inventory/managedObjects/update
```

```json5 title="Payload"
{
  "type": "configured-device-type"
}
```


## Operations/Commands

Operations from Cumulocity are mapped to their equivalent commands in %%te%% format.

### Supported Operations/Capabilities

All the supported operations of all registered devices can be derived from the metadata messages
linked to their respective `cmd` topics with a simple subscription as follows:

```sh
mosquitto_sub -v -t 'te/+/+/+/+/cmd/+'
```

If that subscription returns the following messages:

``` text title="Output"
[te/device/main///cmd/restart] {}
[te/device/main///cmd/log_upload] { "types": ["tedge-agent", "mosquitto"] }
[te/device/child01///cmd/config_snapshot] { "types": ["mosquitto", "tedge", "collectd"] }
[te/device/child01///cmd/config_update] { "types": ["mosquitto", "tedge", "collectd"] }
```

The `cmd` metadata registered for both the `main` device and the child device `child01`
are mapped to the following supported operations messages:

```text
[c8y/s/us] 114,c8y_Restart,c8y_LogfileRequest
[c8y/s/us/<main-device-id>:device:child01] 114,c8y_UploadConfigFile,c8y_DownloadConfigFile
```

The operation specific metadata like `types` for `log_upload`, `config_snapshot` and `config_update`
are also mapped to their corresponding _supported logs_ and _supported configs_ messages as follows:

```text
[c8y/s/us] 118,"tedge-agent", "mosquitto"
[c8y/s/us/<main-device-id>:device:child01] 119,"mosquitto", "tedge", "collectd"
```

### Device Restart

#### Request

<div class="code-indent-left">

**Cumulocity (input)**

```text title="Topic"
c8y/s/ds
```

```csv title="Payload"
510,<main-device-id>
```

</div>

<div class="code-indent-right">

**%%te%% (output)**

```text title="Topic"
te/device/main///cmd/restart/<cmd-id>
```

```json5 title="Payload"
{
    "status": "init"
}
```

</div>

#### Response

Even though operations on tedge can have different kinds of `status` for each operation type,
the mapper recognizes and maps only the following `status` values as follows:

<table style={{width:'100%'}}>
<tr>
  <th>%%te%% (input)</th>
  <th>Cumulocity (output)</th>
</tr>

<tr>
  <td>

```text title="Topic"
te/device/main///cmd/restart/<cmd-id>
```

```json5 title="Payload"
{
    "status": "executing"
}
```

  </td>

  <td>

```text title="Topic"
c8y/s/us
```

```text title="Payload"
501,c8y_Restart
```

  </td>
</tr>

<tr>
  <td>

```text title="Topic"
te/device/main///cmd/restart/<cmd-id>
```

```json5 title="Payload"
{
    "status": "successful"
}
```

  </td>

  <td>

```text title="Topic"
c8y/s/us
```

```text title="Payload"
503,c8y_Restart
```

  </td>
</tr>

<tr>
  <td>

```text title="Topic"
te/device/main///cmd/restart/<cmd-id>
```

```json5 title="Payload"
{
    "status": "failed"
}
```

  </td>

  <td>

```text title="Topic"
c8y/s/us
```

```text title="Payload"
502,c8y_Restart
```

  </td>
</tr>

</table>

All other `status` values are just ignored.

### Restart: Child device

<div class="code-indent-left">

**Cumulocity (input)**

```text title="Topic"
c8y/s/ds
```

```csv title="Payload"
510,<main-device-id>:device:child01
```

</div>

<div class="code-indent-right">

**%%te%% (output)**

```text title="Topic"
te/device/child01///cmd/restart
```

```json5 title="Payload"
{}
```

</div>

### Software Update

<div class="code-indent-left">

**Cumulocity (input)**

```text title="Topic"
c8y/s/ds
```

```csv title="Payload"
528,<main-device-id>,nodered::debian,1.0.0,<c8y-url>,install,collectd,5.7,,install,rolldice,1.16,,delete
```

</div>

<div class="code-indent-right">

**%%te%% (output)**

```text title="Topic"
te/device/main///cmd/software_update/<cmd_id>
```

```json5 title="Payload"
{
    "status": "init",
    "updateList": [
        {
            "type": "debian",
            "modules": [
                {
                    "name": "nodered",
                    "version": "1.0.0",
                    "url": "<tedge-url>",
                    "action": "install"
                },
                {
                    "name": "collectd",
                    "version": "5.7",
                    "action": "install"
                },
                {
                    "name": "rolldice",
                    "version": "1.16",
                    "action": "remove"
                }
            ]
        }
    ]
}
```

</div>

Where the `collectd` binary from the `<c8y-url>` is downloaded to the tedge file transfer repository by the mapper,
and the local `<tedge-url>` of that binary is included in the mapped request.

### Configuration Snapshot

<div class="code-indent-left">

**Cumulocity (input)**

```text title="Topic"
c8y/s/ds
```

```csv title="Payload"
526,<main-device-id>,collectd
```

</div>

<div class="code-indent-right">

**%%te%% (output)**

```text title="Topic"
te/device/main///cmd/config_snapshot/<cmd_id>
```

```json5 title="Payload"
{
    "status": "init",
    "type": "collectd",
    "url": "<tedge-url>"
}
```

</div>

Where the `url` is the target URL in the tedge file transfer repository to which the config snapshot must be uploaded.

### Configuration Update

<div class="code-indent-left">

**Cumulocity (input)**

```text title="Topic"
c8y/s/ds
```

```csv title="Payload"
524,<main-device-id>,<c8y-url>,collectd
```

</div>

<div class="code-indent-right">

**%%te%% (output)**

```text title="Topic"
te/device/main///cmd/config_update/<cmd_id>
```

```json5 title="Payload"
{
    "status": "init",
    "type": "collectd",
    "url": "<tedge-url>"
}
```

</div>

Where the `collectd` configuration binary from the `<c8y-url>` is downloaded to the tedge file transfer repository by the mapper,
and the local `<tedge-url>` of that binary is included in the mapped request.

### Log Upload

<div class="code-indent-left">

**Cumulocity (input)**

```text title="Topic"
c8y/s/ds
```

```csv title="Payload"
522,<main-device-id>,tedge-agent,2013-06-22T17:03:14.000+02:00,2013-06-22T18:03:14.000+02:00,ERROR,1000
```

</div>

<div class="code-indent-right">

**%%te%% (output)**

```text title="Topic"
te/device/main///cmd/log_upload/<cmd_id>
```

```json5 title="Payload"
{
  "status": "init",
  "type": "tedge-agent",
  "tedgeUrl": "<tedge-url>",
  "dateFrom": "2013-06-22T17:03:14.000+02:00",
  "dateTo": "2013-06-23T18:03:14.000+02:00",
  "searchText": "ERROR",
  "lines": 1000
}
```

</div>

Where the `url` is the target URL in the tedge file transfer repository to which the config snapshot must be uploaded.
