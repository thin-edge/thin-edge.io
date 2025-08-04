---
title: Custom Fragments
tags: [Operate, Cumulocity]
description: Publishing custom fragments/properties to Cumulocity
---

%%te%% supports update custom fragments (also known as properties) on the device's digital twin representation in Cumulocity.

## Default fragments

By default, the device will send the following information to Cumulocity. The information makes it easy to identify devices which are using %%te%% in your fleet.

```json
{
  "type": "thin-edge.io",
  "c8y_Agent": {
    "name": "thin-edge.io",
    "url": "https://thin-edge.io",
    "version": "1.0.0"
  }
}
```

The default value for the `type` property can be changed using the `tedge config` command as follows:

```sh
sudo tedge config set device.type VALUE
```

For example, you can set the `type` property to `edge_gateway` using:

```sh
sudo tedge config set device.type edge_gateway
```

## Custom fragments

Additional fragments can be added to the device by either publishing to a give MQTT topic, or via a file based method. Each section describes what data and when to use it.

:::tip
In order to display your custom fragments in Cumulocity's Device Management UI, you have to add them to the "Device Data" Widget of your Device's Info Page. See the [Device Details Info](https://cumulocity.com/docs/device-management-application/viewing-device-details/#info) for reference.
:::

### MQTT Dynamic Fragments {#dynamic-fragments}

%%te%% offers an MQTT topic which can be used to publish data to custom fragments for a device, child devices or services.

* Values which change over time
* Update values without having to restart any services

The following shows an example of publishing the name and version of the Operating System to the `os_Version` fragment for the main device.

```sh te2mqtt
tedge mqtt pub te/device/main///twin/os_Version '{
    "name": "Poky (Yocto Project Reference Distro)",
    "version": "4.0.15 (kirkstone)"
}'
```

The example above will result in the following fragment being added to the device's digital twin (e.g. *Managed Object*) in Cumulocity.

```json5
{
  // ... other fragments are left out for simplicity
  "os_Version": {
    "name": "Poky (Yocto Project Reference Distro)",
    "version": "4.0.15 (kirkstone)"
  }
}
```

### File-based Static Fragments {#static-fragments}

The file based approach is intended for static information, e.g. build date, or a custom image type assigned to the device. The values are only published on startup of the **tedge-mapper-c8y** service.

If you wish to add more fragments to Cumulocity, you can do so by populating `/etc/tedge/device/inventory.json`.

An example `inventory.json` looks something like this:

```json title="file: /etc/tedge/device/inventory.json"
{
  "c8y_Hardware": {
    "model": "BCM2708",
    "revision": "000e",
    "serialNumber": "00000000e2f5ad4d"
  }
}
```

To see the changes you need to restart the tedge-agent.
If you're using systemctl you can do: 

```sh
sudo systemctl restart tedge-agent
```

In the Cumulocity UI this will looks something like this:

<p align="center">
    <img
        src={require('../../images/c8y_custom_fragments.png').default}
        alt="Cumulocity custom fragments"
        width="40%"
    />
</p>

The `tedge-agent` publishes fragments in this file to their corresponding twin topics as retained messages.
For example, the above `inventory.json` file is processed as follows:

```sh te2mqtt
tedge mqtt pub --retained te/device/main///twin/c8y_Hardware '{
  "model": "BCM2708",
  "revision": "000e",
  "serialNumber": "00000000e2f5ad4d"
}'
```

Since these entries are persistent retained messages, when entries are removed from the `inventory.json` file,
the corresponding twin entries must also be cleared explicitly from the broker as follows:

```sh te2mqtt
tedge mqtt pub --retained  te/device/main///twin/c8y_Hardware ''
```

For information on which fragments Cumulocity supports please see the
[Cumulocity API docs](https://cumulocity.com/docs/device-integration/fragment-library/).
