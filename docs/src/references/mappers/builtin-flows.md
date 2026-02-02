---
title: Builtin mapping rules
tags: [Reference, Flows, Mappers, Cloud]
sidebar_position: 3
---

import ProposalBanner from '@site/src/components/ProposalBanner'

<ProposalBanner/>

## Concepts

The out-of-the-box mappers for Cumulocity, Azure, and AWS are defined as [__tedge flows__](./flows.md).
Their definitions can be customized in the same way as any user-defined flow.

- These definitions are persisted on disk alongside the user provided flows for this mapper
- Each mapper generates two files:
  - the builtin flow definition with a `.toml` extension
  - a copy of the generated definition with a `.toml.template` extension

  The builtin flow definition can be customised by the user.
  The template is intended to be used as a reference when the builtin definition is updated.

- This flow defines the message transformations applied by the mapper,
  regardless of whether the user customises the behaviour

- When updating a builtin flow, the user can:
  - Change step configurations
  - Add or remove steps
  - Combine builtin transformations with custom ones implemented in JavaScript

- A builtin flow can also be disabled and fully replaced by user-defined flows.

## The Cumulocity mapper

The behavior of the Cumulocity mapper is driven by a set of flows located in `/etc/tedge/mappers/c8y/flows/`:

```
$ ls -lh /etc/tedge/mappers/c8y/flows/*.toml
-rw-r--r-- 1 tedge   tedge    394 Jan 23 16:50 alarms.toml
-rw-r--r-- 1 tedge   tedge    386 Jan 29 13:17 events.toml
-rw-r--r-- 1 tedge   tedge    299 Jan 29 13:17 health.toml
-rw-r--r-- 1 tedge   tedge    455 Jan 29 13:17 measurements.toml
-rw-r--r-- 1 tedge   tedge    100 Jan 29 13:17 units.toml
```

These files can be customized at different levels:

- Using the `tedge config` command to tune parameters such as the maximum payload of messages sent to Cumulocity
- Editing the builtin flow definitions, to improve, add or remove transformation steps
- Adding user-defined flows
- Removing builtin flows

### Tuning flows with tedge config

The flow definitions of the Cumulocity mapper can be tuned using the `tedge config` command and the following settings:

- `c8y.topics`: the mapper input topics
  - The default is for the mapper to subscribe to measurements `te/+/+/+/+/m/+`, events `te/+/+/+/+/e/+`, 
    alarms `te/+/+/+/+/a/+` and health status `te/+/+/+/+/status/health`, leading each to a specific flow definition.
  - Removing one of those (say `tedge config remove c8y.topics "te/+/+/+/+/m/+"`), removes the associated flow
    (in that case `/etc/tedge/mappers/c8y/flows/measurements.toml`). 
- `c8y.bridge.topic_prefix`: the local MQTT bridge topic prefix  (`c8y` by default)
  - Messages published on this local topic are forwarded by the bridge to Cumulocity 
- `c8y.mapper.mqtt.max_payload_size`
  - Any message with a larger payload (after transformation) will be rejected
  - In the case of events, larger events are published to Cumulocity over HTTP

:::note
For a change of one of these settings to be effective, the Cumulocity mapper has to be restarted.

For instance, to remove the builtin measurement flow:

```
$ sudo tedge config remove c8y.topics "te/+/+/+/+/m/+"
$ sudo systemctl restart tedge-mapper-c8y
```

On restart, the mapper will have removed the `/etc/tedge/mappers/c8y/flows/measurements.toml` file.
:::

### Editing builtin flows

The builtin flows can be edited by users adding extra transformation steps.

For instance, the builtin flow for measurements is defined by the file `/etc/tedge/mappers/c8y/flows/measurements.toml`,
with the default content as follows:

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+", "te/device/main/service/tedge-mapper-c8y/status/entities"]

steps = [
    { builtin = "add-timestamp", config = { property = "time", format = "unix", reformat = false } },
    { builtin = "cache-early-messages", config = { topic_root = "te" } },
    { builtin = "into_c8y_measurements", config = { topic_root = "te" } },
    { builtin = "limit-payload-size", config = { max_size = 16184 } },
]

[output.mqtt]
topic = "c8y/measurement/measurements/create"

[errors.mqtt]
topic = "te/errors"
```

Any modification to this file (adding, editing, removing a step as well as editing input and output)
is dynamically reloaded and immediately made effective by the mapper.

As an example, by default, the Cumulocity mapper caches measurements, events and alarms when received too soon,
i.e. before the source child-device or service has been properly registered.
These messages are cached till their source has been properly registered.
The behavior is implemented by the `cache-early-messages` builtin transformation step.
If this behavior is not desired, one can simply remove that step from the `measurements.toml` flow definition.
A child device will have then to be properly registered for its measurements to be forwarded to Cumulocity.

### Flow definition templates 

Each builtin flow `.toml` definition has a companion file with a `.toml.template` extension.

```
$ ls -lh /etc/tedge/mappers/c8y/flows/
-rw-r--r-- 1 tedge   tedge    394 Jan 23 16:50 alarms.toml
-rw-r--r-- 1 tedge   tedge    394 Jan 23 16:50 alarms.toml.template
-rw-r--r-- 1 tedge   tedge    386 Jan 29 13:17 events.toml
-rw-r--r-- 1 tedge   tedge    386 Jan 29 13:17 events.toml.template
-rw-r--r-- 1 tedge   tedge    299 Jan 29 13:17 health.toml
-rw-r--r-- 1 tedge   tedge    299 Jan 29 13:17 health.toml.template
-rw-r--r-- 1 tedge   tedge    455 Jan 29 13:17 measurements.toml
-rw-r--r-- 1 tedge   tedge    455 Jan 29 13:17 measurements.toml.template
-rw-r--r-- 1 tedge   tedge    100 Jan 29 13:17 units.toml
-rw-r--r-- 1 tedge   tedge    100 Jan 29 13:17 units.toml.template
```

These companion files are generated by the Cumulocity mapper on start and serve two purposes:

- for users, they provide the flow definitions provided out-of-the-box by %%te%% independently of their editing
- for the mapper, they act as witnesses and help detecting that a flow definition has been locally edited.

:::note 
To prevent %%te%% to reset a customized flow, the `template` file must be kept unchanged.

To restore the former behavior of a builtin flow, simply copy its template.
:::

### Adding and removing flows

If flow definitions are provided along the builtin flows in the `/etc/tedge/mappers/c8y/flows/` directory,
then these flows are loaded by the Cumulocity mapper.

Builtin flows can also be disabled by replacing the `.toml` definitions
with files with the same name and a `.toml.disabled` extension

```
# Disable the Cumulocity mapper builtin flow for alarms:
$ mv /etc/tedge/mappers/c8y/flows/alarms.toml /etc/tedge/mappers/c8y/flows/alarms.toml.disabled
```

Alternatively, a builtin flow can be disabled by simply removing its definition
and keeping the associated `.toml.template` file as a witness.

:::note
If the template file is removed, it will be recreated by the mapper on the next restart.
:::

## The Azure mapper

The Azure mapper behavior is defined by a builtin flow located at `/etc/tedge/mappers/az/flows/mea.toml`:

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+", "te/+/+/+/+/e/+", "te/+/+/+/+/a/+", "te/+/+/+/+/status/health"]

steps = [
    { builtin = "skip-mosquitto-health-status" },
    { builtin = "add-timestamp", config = { property = "time", format = "unix", reformat = true } },
    { builtin = "limit-payload-size", config = { max_size = 262144 } },
]

output.mqtt.topic = "az/messages/events/"
errors.mqtt.topic = "te/errors"
```

This file can be tuned using the `tedge config` command and the following settings:

- `az.topics`: the mapper input topics
- `az.mapper.timestamp`: whether the mapper should add a timestamp or not
- `az.mapper.timestamp_format`: the date format to be used for the timestamp (`unix` or `rfc-3339`)
- `az.mapper.mqtt.max_payload_size`: the maximum payload for a message (`262144` by default)
- `az.bridge.topic_prefix`: the prefix used for the bridge local MQTT topic  (`az` by default)

:::note
For a change of one of these settings to be effective, the Azure mapper has to be restarted:
```
$ sudo tedge config set az.mapper.timestamp_format rfc-3339
$ sudo systemctl restart tedge-mapper-az
```
:::

This file can also be manually edited to:
- Change step configurations
- Add or remove steps
- Substitute a JavaScript implementation for the builtin transformation.

:::note
If the builtin flow is updated, then the Azure mapper will not override its content,
even if some `tedge config` settings have been updated. In such a case, the Azure mapper only
update its flow template, i.e. the file `/etc/tedge/mappers/az/flows/mea.toml.template`.
:::

If other flow definitions are provided along the builtin flow in the `/etc/tedge/mappers/az/flows/` directory,
then these flows are loaded by the Azure mapper.

Finally, the builtin flow is disabled when replaced by a file with the same name and a `.toml.disabled` extension

```
# Disable the Azure mapper builtin flow:
$ mv /etc/tedge/mappers/az/flows/mea.toml /etc/tedge/mappers/az/flows/mea.toml.disabled
```

Alternatively, a builtin flow can be disabled by simply removing its definition
and keeping the associated `.toml.template` file as a witness.

## The AWS mapper

The AWS mapper behavior is defined by a builtin flow located at `/etc/tedge/mappers/aws/flows/mea.toml`:

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+", "te/+/+/+/+/e/+", "te/+/+/+/+/a/+", "te/+/+/+/+/status/health"]

steps = [
  { builtin = "skip-mosquitto-health-status" },
  { builtin = "add-timestamp", config = { property = "time", format = "unix", reformat = true } },
  { builtin = "limit-payload-size", config = { max_size = 131072 } },
  { builtin = "set-aws-topic", config = { prefix = "aws" } },
]

errors.mqtt.topic = "te/errors"

```

This file can be tuned using the `tedge config` command and the following settings:

- `aws.topics`: the mapper input topics
- `aws.mapper.timestamp`: whether the mapper should add a timestamp or not
- `aws.mapper.timestamp_format`: the date format to be used for the timestamp (`unix` or `rfc-3339`)
- `aws.mapper.mqtt.max_payload_size`: the maximum payload for a message (`131072` by default)
- `aws.bridge.topic_prefix`: the prefix used for the bridge local MQTT topic  (`aws` by default)

:::note
For a change of one of these settings to be effective, the AWS mapper has to be restarted:
```
$ sudo tedge config set aws.mapper.timestamp_format rfc-3339
$ sudo systemctl restart tedge-mapper-aws
```
:::

This file can also be manually edited to:
- Change step configurations
- Add or remove steps
- Substitute a JavaScript implementation for the builtin transformation.

:::note
If the builtin flow is updated, then the AWS mapper will not override its content,
even if some `tedge config` settings have been updated. In such a case, the AWS mapper only
update its flow template, i.e. the file `/etc/tedge/mappers/aws/flows/mea.toml.template`.
:::

If other flow definitions are provided along the builtin flow in the `/etc/tedge/mappers/aws/flows/` directory,
then these flows are loaded by the AWS mapper.

Finally, the builtin flow is disabled when replaced by a file with the same name and a `.toml.disabled` extension

```
# Disable the AWS mapper builtin flow:
$ mv /etc/tedge/mappers/aws/flows/mea.toml /etc/tedge/mappers/aws/flows/mea.toml.disabled
```

Alternatively, a builtin flow can be disabled by simply removing its definition
and keeping the associated `.toml.template` file as a witness.