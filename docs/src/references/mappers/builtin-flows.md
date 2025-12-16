---
title: Builtin mapping rules
tags: [Reference, Flows, Mappers, Cloud]
sidebar_position: 3
---

import ProposalBanner from '@site/src/components/ProposalBanner'

<ProposalBanner/>

## Concepts

The behavior of the mappers provided out-of-the-box by %%te%% for Cumulocity, Azure and AWS, 
is defined using [__tedge flows__](./flows.md) which definition can be adapted exactly as user defined flows:

- The definitions of the builtin flows of a mapper are persisted on disk along the user provided flows for this mapper.
- Each builtin flow is materialized by two files:
  - a flow definition with a `.toml` extension, which can be updated by users and defines the behavior of the mapper
  - a copy of the builtin definition with a `.toml.template` extension,
    which is not supposed to be updated and can be used as a reference when the active definition is updated.
- These flow definitions - user-defined or builtin, updated or not, define the message transformations applied by the mapper.
- When updating a builtin flow, the user can:
  - Change step configurations
  - Add, remove steps
  - Combine builtin transformations with custom ones implemented in JavaScript.
- A builtin flow can also be disabled and fully replaced by user-defined flows.

:::note
Currently, only the Azure mapper is defined using flows.
:::

## The Azure mapper

The Azure mapper behavior is defined by a builtin flow located at `/etc/tedge/az/flows/mea.toml`:

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+", "te/+/+/+/+/e/+", "te/+/+/+/+/a/+", "te/+/+/+/+/status/health"]

steps = [
    { builtin = "skip-mosquitto-health-status" },
    { builtin = "add-timestamp", config = { property = "time", format = "unix", reformat = true } },
    { builtin = "cap-payload-size", config = { max_size = 262144 } },
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
$ sudo systemctl restart tedge-mapper-c8y
```
:::

This file can also be manually edited to:
- change step configurations
- add, remove steps
- substitute a JavaScript implementation for the builtin transformation.

:::note
If the builtin flow is updated, then the Azure mapper will not override its content,
even if some `tedge config` settings have been updated. In such a case, the Azure mapper only
update its flow template, i.e. the file `/etc/tedge/az/flows/mea.toml.template`.
:::

If other flow definitions are provided along the builtin flow in the `/etc/tedge/az/flows/` directory,
then these flows are loaded by the Azure mapper.

Finally, the builtin flow is disabled when replaced by a file with the same name and a `.toml.disabled` extension

```
# Disable the Azure mapper builtin flow:
$ mv /etc/tedge/az/flows/mea.toml /etc/tedge/az/flows/mea.toml.disabled
```