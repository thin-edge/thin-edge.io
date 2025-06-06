---
title: Generic Mapper
tags: [Reference, Mappers, Cloud]
sidebar_position: 2
draft: true
---

import ProposalBanner from '@site/src/components/ProposalBanner'

<ProposalBanner/>

:::note
This section is actually a design document.
It includes a reference guide for the POC, but also proposes a plan toward a generic mapper.
:::

## Motivation

In theory, %%te%% users can implement customized mappers to transform data published on the MQTT bus
or to interact with the cloud. In practice, they don't.
Implementing a mapper is costly while what is provided out-the-box by %%te%% already meets most requirements.
The need is not to write new mappers but to adapt existing ones.

The aim of the generic mapper it to let users extend and adapt the mappers with their own filtering and mapping rules,
leveraging the core mapping rules and mapper mechanisms (bridge connections, HTTP proxies, operations).

## Vision

The %%te%% mappers for Cumulocity, Azure, AWS and Collectd are implemented on top of a so-called generic mapper
which is used to drive all MQTT message transformations.
- Transformations are implemented as pipelines consuming MQTT messages, feeding a chain of filters and producing MQTT messages.
  - `MQTT sub| filter-1 | filter-2 | ... | filter-n | MQTT pub`
- A pipeline can combine builtin and user-provided filters.
- The user can configure all the transformations used by a mapper,
  editing MQTT sources, pipelines, filters and MQTT sinks.
- By contrast with the current implementation, where the translation of measurements from %%te%% JSON to Cumulocity JSON
  is fully hard-coded, with the generic mapper a user can re-use the core of this transformation while adding customized steps:
  - consuming measurement from a non-standard topic
  - filtering out part of the measurements
  - normalizing units
  - adding units read from some config
  - producing transformed measurements on a non-standard topic.

## POC reference

- The generic mapper loads pipeline and filters stored in `/etc/tedge/gen-mapper/`.
- A pipeline is defined by a TOML file with `.toml` extension.
- A filter is defined by a Javascript file with `.js` extension.
- The definition of pipeline must provide a list of MQTT topics to subscribe to.
  - The pipeline will be feed with all the messages received on these topics.
- A pipeline definition also provides a list of stages.
  - Each stage is built from a javascript and is possibly given a config (arbitrary json that will be passed to the script)
  - Each stage can also subscribe to a list of MQTT topics (which messages will be passed to the script to update its config)

```toml
input_topics = ["te/+/+/+/+/m/+"]

stages = [
    { filter = "add_timestamp.js" },
    { filter = "drop_stragglers.js", config = { max_delay = 60 } },
    { filter = "te_to_c8y.js", meta_topics = ["te/+/+/+/+/m/+/meta"] }
]
```

- A filter has to export at least one `process` function.
  - This function is called for each message to be transformed
  - The arguments passed to the function are:
    - The current time as `{ seconds: u64, nanoseconds: u32 }` 
    - The message `{ topic: string, payload: string }`
    - The config as read from the pipeline config or updated by the script
  - The function is expected to return zero, one or many transformed messages `[{ topic: string, payload: string }]`
  - An exception can be thrown if the input message cannot be transformed.
- A filter can also export an `update_config` function
  - This function is called on each message received on the `meta_topics` as defined in the config.
  - The arguments are:
    - The message to be interpreted as a config update `{ topic: string, payload: string }`
    - The current config
   - The returned value (an arbitrary JSON value) is then used as the new config for the filter.
- A filter can also export a `tick` function
  - This function is called at a regular pace with the current time and config.
  - The filter can then return zero, one or many transformed messages
  - By sharing an internal state between the `process` and `tick` functions,
    the filter can implement aggregations over a time window.

## Ideas and alternatives

### Combine builtin and user-provided filters

### Several kinds of filters

The POC expects the filter to implement a bunch of functions. This gives a quite expressive interface
(filtering, mapping, splitting, dynamic configuration, aggregation over time windows), but at the cost of some complexity.

- `process(t: Timestamp, msg: Message, config: Json) -> Vec<Message>`
- `tick(t: Timestamp) -> Vec<Message>`
- `update_config(msg: Message, config: Json) -> Json`

An alternative is to let the user implement more specific functions with simpler type signatures:

- `filter(msg: Message, config: Json) -> bool`
- `map(msg: Message, config: Json) -> Message`
- `filter_map(msg: Message, config: Json) -> Option<Message>`
- `flat_map(msg: Message, config: Json) -> Vec<Message>`

### Inline definition

### Use JSON for all parameters

### Feed filters with message excerpts as done for the workflows

### Test tools

```shell
$ tedge mapping test [pipeline.toml | filter.js] topic message
```

One should be able to pipe `tedge mqtt sub` and `tedge mapping test`

```shell
$ tedge mqtt sub 'te/+/+/+/+/m/+' | tedge mapping test te-to-c8y.js
```