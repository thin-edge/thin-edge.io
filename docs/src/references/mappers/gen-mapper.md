---
title: Extensible mapper and user-provided Flows
tags: [Reference, Mappers, Cloud]
sidebar_position: 2
draft: true
---

import ProposalBanner from '@site/src/components/ProposalBanner'

<ProposalBanner/>

## Concepts

Users can extend and adapt the built-in mappers for Cumulocity, Azure and AWS
with their own filtering and message transformation rules,
leveraging the core mapping rules and mapper mechanisms (bridge connections, HTTP proxies, operations).

As an example, users can now adapt to their use cases the translation of measurements from %%te%% JSON to Cumulocity JSON:
  - consuming measurements from a non-standard topic
  - filtering out part of the measurements
  - normalizing units
  - adding units read from device config
  - producing transformed measurements on a non-standard topic.

The behavior of a mapper is defined by a set of *connectors*, *flows*, *steps* and transformation *scripts*
which rule how to consume, transform and produce MQTT messages.

- A *step* function transforms one input message into zero, one or more output messages.
  - Steps are effect-free functions, with no access to MQTT, HTTP or the file-system.
  - The focus is on message transformation, format conversion, content extraction and completion as well as filtering and redacting.
- A *connector* is used by the mapper to consume messages from and produce messages to.
  - MQTT is the primary message source and target, but overtime others can be added.
  - Connectors can be seen as streams of messages all with the same shape so they can be processed by any step.
- A *flow* applies a chain of transformation *steps* to input messages producing fully processed output messages.
  - The *flows* put things in motion, actually interacting with the system, consuming and producing messages.
  - Messages received on a flow are passed to the first step; and the transformed messages, if any,
    are pushed to the subsequent steps upto the output connector.
- A flow can combine builtin and user-provided steps.
  - Builtin steps provide generic building blocks such as %%te%% JSON translation into Cumulocity JSON.
  - Users can implement specific steps using JavaScript or TypeScript to refine transformations to their use cases. 
- If some message transformations can be fully defined only from the input message, most require a *context*.
  - What is the Cumulocity internal id of the device? What are the units used by a sensor? Does the location of the device matter?
  - Such a context can only be specific and has to be built from various sources, configuration, metadata and capability messages.  
  - For that purpose, %%te%% maintain a context object which is
    - created, cached and populated by %%te%% using configuration data,
    - passed to all invocations of transformation steps,
    - enriched by some flows and steps with context info extracted from metadata and capability messages,
    - used by all flows and steps to adapt their behavior
- %%te%% provides some support to steps aggregating messages over time windows.
  - For each aggregating step, the mapper persists a state (a JSON object)
    which can be updated by the step function on each message and at regular intervals
    to produce transformed messages on time-window boundaries.

## Step API

A transformation *scripts* is a JavaScript or TypeScript module that exports:

- at least a function `onMessage`, aimed to transform one input message into zero, one or more output messages
- possibly a function `onInterval`, called at regular intervals to produce aggregated messages.




## Flow configuration

- The generic mapper loads flows and steps stored in `/etc/tedge/gen-mapper/`.
- A flow is defined by a TOML file with `.toml` extension.
- A step is defined by a JavaScript file with `.js` extension.
  - This can also be a TypeScript module with a `.ts` extension.
- The definition of flows must provide a list of MQTT topics to subscribe to.
  - The flow will be feed with all the messages received on these topics.
- A flow definition provides a list of steps.
  - Each step is built from a javascript and is possibly given a config (arbitrary json that will be passed to the script)
  - Each step can also subscribe to a list of MQTT topics (which messages will be passed to the script to update its config)

```toml
input_topics = ["te/+/+/+/+/m/+"]

steps = [
    { script = "add_timestamp.js" },
    { script = "drop_stragglers.js", config = { max_delay = 60 } },
    { script = "te_to_c8y.js", meta_topics = ["te/+/+/+/+/m/+/meta"] }
]
```

## POC API

- A flow script has to export at least one `onMessage` function.
  - `onMessage(t: Timestamp, msg: Message, config: Json) -> Vec<Message>` 
  - This function is called for each message to be transformed
  - The arguments passed to the function are:
    - The current time as `{ seconds: u64, nanoseconds: u32 }` 
    - The message `{ topic: string, payload: string }`
    - The config as read from the flow config or updated by the script
  - The function is expected to return zero, one or many transformed messages `[{ topic: string, payload: string }]`
  - An exception can be thrown if the input message cannot be transformed.
- A flow script can also export an `onConfigUpdate` function
  - This function is called on each message received on the `meta_topics` as defined in the config.
  - The arguments are:
    - The message to be interpreted as a config update `{ topic: string, payload: string }`
    - The current config
   - The returned value (an arbitrary JSON value) is then used as the new config for the flow script.
- A flow script can also export a `onInterval` function
  - This function is called at a regular pace with the current time and config.
  - The flow script can then return zero, one or many transformed messages
  - By sharing an internal state between the `onMessage` and `onInterval` functions,
    the flow script can implement aggregations over a time window.
    When messages are received they are pushed by the `onMessage` function into that state
    and the final outcome is extracted by the `onInterval` function at the end of the time window.
