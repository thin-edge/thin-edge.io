---
title: Cumulocity Mapper
tags: [Reference, Mappers, Cloud]
sidebar_position: 1
---

# Cumulocity Mapper

The Cumulocity mapper, referred to as `c8y-mapper` in the rest of this document,
maps data in [Thin Edge format](../mqtt-api.md) into their equivalent [Cumulocity format](https://cumulocity.com/guides/reference/smartrest-two/#smartrest-two).


## Registration

Cumulocity keeps the record of all the registered devices and their associated metadata in its inventory.
The `c8y-mapper` creates and maintains inventory entries for all the devices and services registered with thin-edge.
The mapper subscribes to the following topics to get a list of all registered devices and services:

```sh
mosquitto_sub -t 'te/+' -t 'te/+/+' -t 'te/+/+/+' -t 'te/+/+/+/+'
```

The registration messages received for child devices and services are mapped as follows:

### Child device

<div class="code-indent-left">

**Thin Edge (input)**

```text title="Topic"
te/device/child01
```

```json5 title="Payload"
{
  "@type": "child-device",
  "displayName": "child01",
  "type": "SmartHomeHub"
}
```

</div>

<div class="code-indent-right">

**Cumulocity IoT (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
101,<main-device-id>:device:child01,SmartHomeHub
```

</div>

Where the `<main-device-id>` is added as the prefix to the external id to avoid id clashes
with devices using the same name in other tedge deployments connected to the same tenant.

### Child device with explicit id

<div class="code-indent-left">

**Thin Edge (input)**

```text title="Topic"
te/device/child01
```

```json5 title="Payload"
{
  "@type": "child-device",
  "@id": "child01",
  "displayName": "child01",
  "type": "SmartHomeHub"
}
```

</div>

<div class="code-indent-right">

**Cumulocity IoT (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
101,child01,SmartHomeHub
```

</div>

Where the provided `@id` is directly used as the external id.

### Nested child device

<div class="code-indent-left">

**Thin Edge (input)**

```text title="Topic"
te/device/nested_child01
```

```json5 title="Payload"
{
  "@type": "child-device",
  "@parent": "te/device/child01",
  "displayName": "nested_child01",
  "type": "BatterySensor"
}
```

</div>


<div class="code-indent-right">

**Cumulocity IoT (output)**

```text title="Topic"
c8y/s/us/<main-device-id>:device:child01
```

```text title="Payload"
101,<main-device-id>:device:nested_child01,nested_child01,BatterySensor
```

</div>

### Main device service

<div class="code-indent-left">

**Thin Edge (input)**

```text title="Topic"
te/device/main/service/nodered
```

```json5 title="Payload"
{
  "@type": "service",
  "displayName": "Node-Red",
  "type": "systemd"
}
```

</div>


<div class="code-indent-right">

**Cumulocity IoT (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
104,<main-device-id>:device:main:service:nodered,systemd,Node-Red,up
```

</div>

### Child device service

<div class="code-indent-left">

**Thin Edge (input)**

```text title="Topic"
te/device/child01/service/nodered
```

```json5 title="Payload"
{
  "@type": "service",
  "displayName": "Node-Red",
  "type": "systemd"
}
```

</div>


<div class="code-indent-right">

**Cumulocity IoT (output)**

```text title="Topic"
c8y/s/us/<main-device-id>:device:child01
```

```text title="Payload"
104,<main-device-id>:device:child01:service:nodered,systemd,Node-Red,up
```

</div>

Where the unique external IDs to be used in the cloud are derived from the entity identifier subtopics,
replacing the `/` characters with `:`.

:::note
The main device is registered with the cloud via the `tedge connect c8y` command execution
and hence there is no mapping done for main device registration messages.
Inventory data updates for the main device are handled differently.
:::

## Telemetry

Telemetry data types like measurements, events and alarms are mapped to their respective equivalents in Cumulocity as follows:

### Measurement

<div class="code-indent-left">

**Thin Edge (input)**

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

**Cumulocity IoT (output)**

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

**Thin Edge (input)**

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

**Cumulocity IoT (output)**

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

**Thin Edge (input)**

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

**Cumulocity IoT (output)**

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

#### Measurement with unit

The unit is a metadata associated with measurements which can be registered as a metadata message for a given measurement type.
If the following metadata message is registered for the `environment` measurement type:

```sh te2mqtt
tedge mqtt pub -r te/device/main///m/environment/meta '{
  "units": {
    "temperature": "°C"
  }
}'
```

Then subsequent messages will be mapped with the registered unit value as follows.

<div class="code-indent-left">

**Thin Edge (input)**

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

**Cumulocity IoT (output)**

```text title="Topic"
c8y/measurement/measurements/create
```

```json5 title="Payload"
{
  "type": "environment",
  "time": "2021-04-22T17:05:26.958340390+00:00",
  "temperature": {
    "temperature": {
      "value": 23,
      "unit": "°C"
    }
  }
}
```

</div>

### Events

<div class="code-indent-left">

**Thin Edge (input)**

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

**Cumulocity IoT (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
400,login_event,"A user just logged in",2021-01-01T05:30:45+00:00
```

</div>

#### Event - Complex

<div class="code-indent-left">

**Thin Edge (input)**

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

**Cumulocity IoT (output)**

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

**Thin Edge (input)**

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

**Cumulocity IoT (output)**

```text title="Topic"
c8y/s/us
```

```text title="Payload"
301,temperature_high,"Temperature is very high",2021-01-01T05:30:45+00:00
```

</div>

#### Alarm - Complex

<div class="code-indent-left">

**Thin Edge (input)**

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

**Cumulocity IoT (output)**

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

## Operations/Commands

Operations from Cumulocity are mapped to their equivalent commands in Thin Edge format.

### Supported Operations/Capabilities

All the supported operations of all registered devices can be derived from the metadata messages
linked to their respective `cmd` topics with a simple subscription as follows:

```sh
mosquitto_sub -v -t 'te/+/+/+/+/cmd/+'
```

If that subscription returns the following messages:

``` text title="Output"
[te/device/main///cmd/restart] {}
[te/device/main///cmd/log_upload] { "supportedTypes": ["tedge-agent", "mosquitto"] }
[te/device/child01///cmd/config_snapshot] { "supportedTypes": ["mosquitto", "tedge", "collectd"] }
[te/device/child01///cmd/config_update] { "supportedTypes": ["mosquitto", "tedge", "collectd"] }
```

The `cmd` metadata registered for both the `main` device and the child device `child01`
are mapped to the following supported operations messages:

```text
[c8y/s/us] 114,c8y_Restart,c8y_LogfileRequest
[c8y/s/us/<main-device-id>:device:child01] 114,c8y_UploadConfigFile,c8y_DownloadConfigFile
```

The operation specific metadata like `supportedTypes` for `log_upload`, `config_snapshot` and `config_update`
are also mapped to their corresponding _supported logs_ and _supported configs_ messages as follows:

```text
[c8y/s/us] 118,"tedge-agent", "mosquitto"
[c8y/s/us/<main-device-id>:device:child01] 119,"mosquitto", "tedge", "collectd"
```

### Device Restart

#### Request

<div class="code-indent-left">

**Cumulocity IoT (input)**

```text title="Topic"
c8y/s/us
```

```csv title="Payload"
510,<main-device-id>
```

</div>

<div class="code-indent-right">

**Thin Edge (output)**

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
  <th>Thin Edge (input)</th>
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

**Cumulocity IoT (input)**

```text title="Topic"
c8y/s/us
```

```csv title="Payload"
510,<main-device-id>:device:child01
```

</div>

<div class="code-indent-right">

**Thin Edge (output)**

```text title="Topic"
te/device/child01///cmd/restart
```

```json5 title="Payload"
{}
```

</div>

### Software Update

<div class="code-indent-left">

**Cumulocity IoT (input)**

```text title="Topic"
c8y/s/us
```

```csv title="Payload"
528,<main-device-id>,nodered::debian,1.0.0,<c8y-url>,install,collectd,5.7,,install,rolldice,1.16,,delete
```

</div>

<div class="code-indent-right">

**Thin Edge (output)**

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

**Cumulocity IoT (input)**

```text title="Topic"
c8y/s/us
```

```csv title="Payload"
526,<main-device-id>,collectd
```

</div>

<div class="code-indent-right">

**Thin Edge (output)**

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

**Cumulocity IoT (input)**

```text title="Topic"
c8y/s/us
```

```csv title="Payload"
524,<main-device-id>,<c8y-url>,collectd
```

</div>

<div class="code-indent-right">

**Thin Edge (output)**

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

**Cumulocity IoT (input)**

```text title="Topic"
c8y/s/us
```

```csv title="Payload"
522,<main-device-id>,tedge-agent,2013-06-22T17:03:14.000+02:00,2013-06-22T18:03:14.000+02:00,ERROR,1000
```

</div>

<div class="code-indent-right">

**Thin Edge (output)**

```text title="Topic"
te/device/main///cmd/log_upload/<cmd_id>
```

```json5 title="Payload"
{
  "status": "init",
  "type": "tedge-agent",
  "url": "<tedge-url>",
  "dateFrom": "2013-06-22T17:03:14.000+02:00",
  "dateTo": "2013-06-23T18:03:14.000+02:00",
  "searchText": "ERROR",
  "maximumLines": 1000
}
```

</div>

Where the `url` is the target URL in the tedge file transfer repository to which the config snapshot must be uploaded.
