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
  - Messages can be consumed from MQTT, files and background processes.
  - Transformed messages can be published over MQTT or appended to files.
- A *flow* applies a chain of transformation *steps* to input messages producing fully processed output messages.
  - The *flows* put things in motion, actually interacting with the system, consuming and producing messages.
  - Messages received on a flow are passed to the first step; and the transformed messages, if any,
    are pushed to the subsequent steps up to the output connector.
- A flow can combine builtin and user-provided steps.
  - Builtin steps provide generic building blocks such as %%te%% JSON translation into Cumulocity JSON.
  - Users can implement specific steps using JavaScript or TypeScript to refine transformations to their use cases. 
- If some message transformations can be fully defined only from the input message, most require a *context*.
  - What is the Cumulocity internal id of the device? What are the units used by a sensor? What is the location of the device?
  - For that purpose, %%te%% maintain a `context` object which is
    - passed to all invocations of transformation steps,
    - structured along 3 namespaces
      - `context.mapper` is shared by all the flows of a mapper
      - `context.flow` is private to the flow, shared by all the steps of that flow
      - `context.script` is private to a script instance and persisted across script reloads
    - created, cached and populated by %%te%% using configuration data,
    - possibly enriched by the flows with data extracted from metadata and capability messages,
    - used by all flows and steps to adapt their behavior
  - The `context` object also provides a `context.config` object
    - which content is specific to each script instance and provided by flows configuration
    - to set values such as thresholds, ranges, durations, units or endpoints
    - giving the users a way to control the behavior of a step without rewriting scripts.
- %%te%% provides some support to steps aggregating messages over time windows.
  - For each aggregating step, the mapper persists a state (a JSON object)
    which can be updated by the step function on each message and at regular intervals
    to produce transformed messages on time-window boundaries.

## Step API

A transformation *script* is a JavaScript or TypeScript module that exports:

- at least, a function `onMessage()`, aimed to transform one input message into zero, one or more output messages,
- possibly, a function `onInterval()`, called at regular intervals to produce aggregated messages,

```ts
interface FlowStep {
    // transform one input message into zero, one or more output messages
    onMessage(message: Message, context: Context): null | Message | Message[],
  
    // called at regular intervals to produce aggregated messages
    onInterval(time: Date, context: Context): null | Message | Message[]
}
```

A message contains the message topic and payload as well as a processing timestamp.
The bytes of the raw message payload are also accessible as an array of unsigned bytes: 

```ts
type Message = {
  topic: string,
  payload: string,
  raw_payload: Uint8Array,
  time: Date
}
```

The `context` object passed to `onMessage()` and `onInterval()` gives scripts and flows a way to share data.

```ts
type Context = {
  // A set of (key, value) pairs shared by all the scripts of a mapper
  mapper: KVStore,
  
  // A set of (key, value) pairs shared by all the scripts of a flow
  flow: KVStore,
  
  // A set of (key, value) pairs private to a script, persisted across module reloads
  script: KVStore,
  
  // A value provided by the flow configuration of that step
  config: any,
}

type KVStore = {
  // List the keys for which this store holds a value  
  keys(): string[]
  
  // Get the value attached to a key (returning null, if none)
  get(key: string): any,
  
  // Set the value attached to a key (removing the key if the provided value is null)
  set(key: string, value: any),
  
  // Remove any value attache to a key
  remove(key: string),
}
```

The `context.config` is an object freely defined by the step module, to provide default values such as thresholds, durations or units.

The `onMessage` function is called for each message to be transformed
  - The arguments passed to the function are:
    - The message `{ topic: string, payload: string, raw_payload: Uint8Array, time: Date }`
    - A context object with the config and state
  - The function is expected to return zero, one or many transformed messages `[{ topic: string, payload: string }]`
  - An exception can be thrown if the input message cannot be transformed.

A flow script can also export a `onInterval` function
  - This function is called at a regular pace with the current time and context.
  - The flow script can then return zero, one or many transformed messages
  - By sharing an internal state between the `onMessage` and `onInterval` functions,
    the flow script can implement aggregations over a time window.
    When messages are received they are pushed by the `onMessage` function into that state
    and the final outcome is extracted by the `onInterval` function at the end of the time window.

## Flow configuration

- The generic mapper loads flows and steps stored in `/etc/tedge/flows/`.
- A flow is defined by a TOML file with `.toml` extension.
- A step is defined by a JavaScript file with an `.mjs` or `.js` extension.
  - This can also be a TypeScript module with a `.ts` extension.
- The definition of flow defines its input, output and error sink as well as a list of transformation steps.
- Each step is built from a javascript and is possibly given a config (arbitrary json that will be passed to the script)

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+"]

steps = [
    { script = "add_timestamp.js" },
    { script = "drop_stragglers.js", config = { max_delay = 60 } },
    { script = "te_to_c8y.js" }
]
```

### Input connectors

Messages can be consumed from MQTT, files and background processes.

An MQTT connector is simply defined by a list of MQTT topics

```toml
# A flow subscribing to all measurement values and meta-data
input.mqtt.topics = ["te/+/+/+/+/m/+", "te/+/+/+/+/m/+/meta"]
```

Messages can also be consumed from a file, each line being interpreted as the payload of a message
which topic is by default the file path.

```toml
# A flow consuming log entries
input.file.path = "/var/log/some-app.log"
```

By default, this file is followed as done by `tail -F`, waiting for new lines to be appended and consumed as messages.
This behavior can be changed, the file being then read at regular intervals.

```toml
[input.file]
path = "/var/log/some-app.log"
topic = "some-app-log"
interval = "1h"
```

Last but not least, messages can be consumed from a command output,
each line being wrapped into a message which topic is the command line or a configured topic name.

```toml
# A flow subscribing to journalctl entries for the agent
[input.process]
topic = "tedge-agent-journalctl"
command = "sudo journalctl --no-pager --follow --unit tedge-agent"
```

As for files, the command output can instead be consumed at regular intervals.

```toml
# A flow publishing new journalctl entries for the agent every hour 
[input.process]
topic = "tedge-agent-journalctl"
command = "journalctl --no-pager --cursor-file=/tmp/tedge-agent-cursor --unit tedge-agent"
interval = "1h"
```

### Output connectors

Transformed messages and errors can be published over MQTT or appended to files.

The default is to publish the transformed messages over MQTT on the topics specified by each message.
And to direct all the errors to a specific topic, the `te/error` topic.

```toml
[output.mqtt]

[errors.mqtt]
topic = "te/error"
```

These defaults can be overridden, by:
- assigning an MQTT topic to the messages (ignoring the topic assigned by the transformation steps)
- accepting only a set of topics (making sure the transformation steps are sending messages to these topics)
- redirecting transformations outcome to a file.

```toml
[output.mqtt]
accept_topics = "c8y/#"

[errors.file]
path = "/var/run/tedge/flows.log"
```

The output of flow can also be directed to the global context aka `context.mapper`.
The main usage is for a flow to store and share metadata received as retained MQTT messages,
so this metadata can be used by other transformation flows;
the canonical example being a context flow populating the context with measurement units to be used by a measurement publisher flow.

```toml
[output.context]
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

## Builtin Objects

%%te%% flows uses the [QuickJS](https://bellard.org/quickjs/) engine and supports [ECMAScript® 2023](https://tc39.es/ecma262/2023/).

Flows and step functions are executed in a sandbox with no access to the system, the disk or the network.

The following builtin objects are exported:

- [`console.log()`](https://developer.mozilla.org/en-US/docs/Web/API/console/log_static)
  - Output log messages to the mapper log
- [`TextDecoder`](https://developer.mozilla.org/en-US/docs/Web/API/TextDecoder)
  - Only `utf-8` is supported 
- [`TextEncoder`](https://developer.mozilla.org/en-US/docs/Web/API/TextEncoder)
