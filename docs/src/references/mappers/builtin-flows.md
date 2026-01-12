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

:::note
Currently, only the Azure mapper is defined using flows.
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