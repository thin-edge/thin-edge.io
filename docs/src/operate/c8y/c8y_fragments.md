---
title: Custom Fragments
tags: [Operate, Cumulocity]
sidebar_position: 2
---

# How to add custom fragments to Cumulocity

## Default fragments

By default your device will send the following information to Cumulocity:

```json
{
  "c8y_Agent": {
    "name": "thin-edge.io",
    "version": "x.x.x"
  }
}
```

You can change the `name` value using the `tedge` command as follows:

```sh
sudo tedge config set device.type VALUE
```

## Custom fragments

If you wish to add more fragments to Cumulocity, you can do so by populating `{base_config_dir}/device/inventory.json`.
The default `base_config_dir` is `/etc/tedge`.
See the link for more information about setting a custom [base_config_dir.](../../references/thin-edge-config-files.md)

An example `inventory.json` looks something like this:

```json title="file: /etc/tedge/device/inventory.json"
{
  "c8y_RequiredAvailability": {
    "responseInterval": 5
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

To see the changes you need to restart the tedge-mapper.
If you're using systemctl you can do: 

```sh
sudo systemctl restart tedge-mapper-c8y
```

In the Cumulocity UI this will looks something like this:

<p align="center">
    <img
        src={require('../../images/c8y_custom_fragments.png').default}
        alt="Cumulocity custom fragments"
        width="40%"
    />
</p>


For information on which fragments Cumulocity supports please see the
[Cumulocity API docs](https://cumulocity.com/guides/10.6.6/reference/device-management/).
