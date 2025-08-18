---
title: Extensible mapper and user-provided Flows
tags: [Reference, Mappers, Cloud]
sidebar_position: 2
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
- A *connector* is used by the mapper to consume messages from a source and produce messages to a sink.
  - MQTT is the primary message source and target, but overtime others can be added.
  - Connectors can be seen as streams of messages all with the same shape so they can be processed by any step.
- A *flow* applies a chain of transformation *steps* to input messages producing fully processed output messages.
  - The *flows* put things in motion, actually interacting with the system, consuming and producing messages.
  - Messages received on a flow are passed to the first step; and the transformed messages, if any,
    are pushed to the subsequent steps up to the output connector.
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

A transformation *script* is a JavaScript or TypeScript module that exports:

- at least, a function `onMessage()`, aimed to transform one input message into zero, one or more output messages,
- possibly, a function `onInterval()`, called at regular intervals to produce aggregated messages,
- possibly, a function `onConfigUpdate()`, used to update the step config.

```ts
interface FlowStep {
    // transform one input message into zero, one or more output messages
    onMessage(message: Message, config: object): null | Message | Message[],
  
    // called at regular intervals to produce aggregated messages
    onInterval(timestamp: Timestamp, config: object): null | Message | Message[],
  
    // update the step config given a config update message
    onConfigUpdate(message: Message, config: object): object
}
```

A message contains the message topic and payload as well as an ingestion timestamp: 

```ts
type Message = {
  topic: string,
  payload: string,
  timestamp: Timestamp
}

type Timestamp = {
  seconds: number,
  nanoseconds: number
}
```

A `config` is an object freely defined by the step module, to provide default values such as thresholds, durations or units.
These values are configured by the flow and can be dynamically updated on reception of config update messages.

The `onMessage` function is called for each message to be transformed
  - The arguments passed to the function are:
    - The message `{ topic: string, payload: string, timestamp: { seconds: u64, nanoseconds: u32 } }`
    - The config as read from the flow config or updated by the script
  - The function is expected to return zero, one or many transformed messages `[{ topic: string, payload: string }]`
  - An exception can be thrown if the input message cannot be transformed.
- If defined and associated in the step config with `meta_topics`, the `onConfigUpdate` function is called on each message received on these `meta_topics`.
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

## Flow configuration

- The generic mapper loads flows and steps stored in `/etc/tedge/flows/`.
- A flow is defined by a TOML file with `.toml` extension.
- A step is defined by a JavaScript file with `.js` extension.
  - This can also be a TypeScript module with a `.ts` extension.
- The definition of flows must provide a list of MQTT topics to subscribe to.
  - The flow will be fed with all the messages received on these topics.
- A flow definition provides a list of steps.
  - Each step is built from a javascript and is possibly given a config (arbitrary json that will be passed to the script)
  - Each step can also subscribe to a list of MQTT meta topics where the metadata about the actual data message is stored
    (e.g, meta topic of a measurement type where its units threshold values are defined).
    The messages received on these topics will be passed to the `onConfigUpdate` letting the script update its config.

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+"]

steps = [
    { script = "add_timestamp.js" },
    { script = "drop_stragglers.js", config = { max_delay = 60 } },
    { script = "te_to_c8y.js", meta_topics = ["te/+/+/+/+/m/+/meta"] }
]
```

## %%te%% flow mapper

The extensible mapper is launched as a regular mapper:

```shell
tedge-mapper flows
```

This mapper:

- loads all the flows defined in `/etc/tedge/flows`
- reloads any flow or script that is created, updated or deleted while the mapper is running
- subscribes to each flow `input.mqtt.topics`, dispatching the messages to the `onMessage` functions
- subscribes to each step `meta_topics`, dispatching the messages to the `onConfigUpdate` functions
- triggers at the configured pace the `onInterval` functions
- publishes memory usage statistics
- publishes flows and steps usage statistics

## %%te%% flow cli

Flows and steps can be tested using the `tedge flows test` command.
- These tests are done without any interaction with MQTT and `tedge-mapper flows`,
  meaning that tests can safely be run on a device in production
- By default, tests run against the flows and scripts used by `tedge-mapper flows`.
  However, a directory of flows under development can be provided using the `--flows-dir <FLOWS_DIR>` option.
- A test can be specific to a flow or script using the `--flow <OPTION>` option.
 
A test can be given a test message on the command line.

```shell
$ tedge flows test te/device/main///m/environment '{ "temperature": 29 }'

[c8y/measurement/measurements/create] {"type":"environment","temperature":{"temperature":29},"time":"2025-08-07T12:47:26.152Z"}
```

Alternatively a test can be given a sequence of messages via its stdin.

```shell
$ echo '[te/device/main///m/environment]' '{ "temperature": 29 }' | tedge flows test

[c8y/measurement/measurements/create] {"type":"environment","temperature":{"temperature":29},"time":"2025-08-07T12:47:26.152Z"}
```

__Note__ that when the input of a test is received from its stdin,
the topic is given using a bracket syntax `[<TOPIC>] <PAYLOAD>`
similar to the output of `tedge mqtt sub` and `tedge flows test` itself.

This can be used to chain tests:

```shell
$ tedge flows test collectd/mandarine/cpu/percent-active '1754571280.572:2.07156308851224' | tedge flows test

[c8y/measurement/measurements/create] {"type":"collectd","time":"2025-08-07T12:54:40.572Z","cpu":{"percent-active":2.07156308851224}}
```
