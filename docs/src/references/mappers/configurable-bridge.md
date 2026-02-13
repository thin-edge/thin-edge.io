---
title: Configurable built-in bridge
tags: [Reference, Mappers, Cloud]
sidebar_position: 3
---

import ProposalBanner from '@site/src/components/ProposalBanner'

<ProposalBanner/>

## Overview

The built-in bridge forwards MQTT messages between the local broker and the cloud. By default, %%te%% provides bridge rules that handle standard communication patterns for Cumulocity, Azure, and AWS. However, you may need to customize this behavior—for example, to bridge additional topics, disable unused rules, or adapt to your cloud tenant's specific topic structure.

When the built-in bridge is enabled, these rules are defined in user-configurable TOML files, giving you full control over which messages are forwarded and how topic names are mapped between local and remote brokers.

## Concepts

When the built-in bridge is enabled, the bridge rules for Cumulocity, Azure and AWS are defined in a user-configurable toml file. This allows the rules to be adapted and extended to fit your needs.

## Syntax

A basic bridge rule looks something like:
```toml
[[rule]]
local_prefix = "c8y/"
remote_prefix = ""
topic = "s/us"
direction = "outbound"
```

This will take messages from the local MQTT broker on `c8y/s/us` and forward them to the cloud on the topic `s/us`.

An example of the remote prefix would be:

```toml
[[rule]]
local_prefix = "aws/"
remote_prefix = "thinedge/my_device_id/"
topic = "cmd/#"
direction = "inbound"
```

This will take messages received from the cloud at `thinedge/my_device_id/cmd/#` and forward them to `aws/cmd/#`.

```toml
[[rule]]
local_prefix = "aws/"
remote_prefix = "$aws/things/my_device_id/"
topic = "shadow/#"
direction = "bidirectional"
```

This takes local messages on `aws/shadow/#` and forwards them to the cloud on `$aws/things/my_device_id/shadow/#`. Because the direction field is set to `bidirectional`, messages received from the cloud on `$aws/things/my_device_id/shadow/#` will be forwarded locally to `aws/shadow/#`.

### Global prefixes

The `local_prefix` and `remote_prefix` fields can be defined per-rule or for the entire file. If a prefix is defined both at the file level and at the rule level, the rule-level prefix takes precedence.

```toml
local_prefix = "aws/"
remote_prefix = "thinedge/my_device_id"

[[rule]]
# Uses the local/remote prefix from the top of the file
topic = "td/#"
direction = "outbound"

[[rule]]
# Uses the local prefix from the top of the file
# Replaces the remote prefix with a different one
remote_prefix = "$aws/things/my_device_id"
topic = "shadow/#"
direction = "bidirectional"
```


### Configuration variable interpolation

You can interpolate variables from tedge config (using the same stringification logic as `tedge config get <key>`) inside `local_prefix`, `remote_prefix` and `topic`:

```toml
[[rule]]
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""
topic = "s/us"
direction = "outbound"
```

```toml
[[rule]]
local_prefix = "${config.aws.bridge.topic_prefix}/"
remote_prefix = "$aws/things/${config.aws.device.id}"
topic = "shadow/#"
direction = "bidirectional"
```

### Template rules

You may wish to define multiple rules using a pattern:

```toml
[[template_rule]]
# Iterating over an array in tedge config
for = "${config.c8y.smartrest.templates}"
topic = "s/uc/${item}"

[[template_rule]]
# We can also iterate over an array of strings
for = ['s', 'c', 'q', 't']
topic = "${item}/us"
```

### Conditional rules

A rule (or an entire file) can be enabled conditionally. Currently this supports boolean config variables:

```toml
remote_prefix = ""
# If `c8y.mqtt_service.enabled` is set to `false`, all rules in this file will be disabled
if = "${config.c8y.mqtt_service.enabled}"

[[rule]]
local_prefix = "${config.c8y.bridge.topic_prefix}/mqtt/out/"
topic = "#"
direction = "outbound"
```

```toml
# We can also make rules conditional
[[rule]]
if = "${connection.auth_method} == 'certificate'"
topic = "s/dat"
direction = "inbound"

[[template_rule]]
if = "${connection.auth_method} == 'password'"
for = ['s', 't', 'q', 'c']
topic = "${item}/ul/#"
direction = "outbound"
```

## Bridge CLI

The `tedge bridge` command provides tools for inspecting and testing bridge rules. This is useful for verifying your configuration, understanding how topics are mapped, and debugging issues with message forwarding.

### `tedge bridge inspect`

To view the active bridge rules for a cloud, use:

```sh
tedge bridge inspect c8y
```

This displays all bridge rules grouped by direction, showing how topics are mapped between the local and remote brokers:

```
Bridge configuration for Cumulocity
Reading from: /etc/tedge/mappers/c8y/bridge

Local -> Remote
  c8y/s/uat                                  ->  s/uat
  c8y/s/ut/#                                 ->  s/ut/#
  c8y/inventory/managedObjects/update/#      ->  inventory/managedObjects/update/#
  c8y/measurement/measurements/create/#      ->  measurement/measurements/create/#
  c8y/s/us/#                                 ->  s/us/#
  ...

Remote -> Local
  s/dat                        ->  c8y/s/dat
  s/ds                         ->  c8y/s/ds
  devicecontrol/notifications  ->  c8y/devicecontrol/notifications
  ...
```

Use `--debug` to also show rules that were skipped due to unmet conditions or empty template loops:

```sh
tedge bridge inspect c8y --debug
```

If you are using named profiles, specify the profile with `--profile`:

```sh
tedge bridge inspect c8y --profile production
```

:::note
`tedge bridge inspect` currently only supports clouds with configurable bridge rules (Cumulocity). For AWS and Azure, the built-in bridge rules are not yet configurable.
:::

### `tedge bridge test`

To check where a specific MQTT topic would be forwarded by the bridge, use:

```sh
tedge bridge test c8y c8y/s/uat
```

```
[local] c8y/s/uat  ->  [remote] s/uat (outbound)
```

The topic can be either a local or a remote topic:

```sh
tedge bridge test c8y s/ds
```

```
[remote] s/ds  ->  [local] c8y/s/ds (inbound)
```

When a topic matches a wildcard rule, the matching rule pattern is also shown:

```sh
tedge bridge test c8y c8y/s/uc/custom-template
```

```
[local] c8y/s/uc/custom-template  ->  [remote] s/uc/custom-template (outbound)
  matched by rule: c8y/s/uc/# -> s/uc/#
```

If no rules match, you'll see a message and the command exits with code 2:

```
No matching bridge rule found for "nonexistent/topic"
```

Wildcard topics are not supported as input, as matching a wildcard against wildcard rules can be ambiguous — for example, `a/+/c` and `a/b/+` both match `a/b/c`, but neither fully contains the other. If you provide a topic containing `#` or `+`, the command exits with an error:

```sh
tedge bridge test c8y 'c8y/#'
```

```
Error: Wildcard characters (#, +) are not supported. Provide a concrete topic to test against.
```

All matching rules are displayed, which can help identify unexpected overlaps if multiple rules match the same topic.

## Template file structure

### Cumulocity

The Cumulocity built-in behavior is configured in the file `/etc/tedge/mappers/c8y/bridge/mqtt-core.toml`.

:::note
To enable this feature, you need to enable the built-in bridge:
```
$ sudo tedge config set mqtt.bridge.built-in true
$ sudo tedge reconnect c8y
```
:::

These files can also be manually edited to add or remove rules

:::note
If the auto-generated bridge configuration is updated, then the Cumulocity mapper will not override its content.
In such a case, the Cumulocity mapper will only update its bridge configuration template, i.e. the file `/etc/tedge/mappers/c8y/bridge/mqtt-core.toml.template`.
:::

If other bridge definitions are provided along the builtin rules in the `/etc/tedge/mappers/c8y/bridge/` directory,
then these bridge rules are loaded by the Cumulocity mapper.

Finally, the builtin configurations can be disabled by replacing the file with the same name and a `.toml.disabled` extension

```
# Disable the Cumulocity mapper builtin rules:
$ mv /etc/tedge/mappers/c8y/bridge/mqtt-core.toml /etc/tedge/mappers/c8y/bridge/mqtt-core.toml.disabled
```

Alternatively, a builtin bridge configuration can be disabled by simply removing its definition
and keeping the associated `.toml.template` file as a witness.
