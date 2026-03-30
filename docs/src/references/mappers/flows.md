---
title: User-defined mapping rules
tags: [Reference, Flows, Mappers, Cloud]
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

- optionally, a function `onStartup()`, invoked once when the flow or step starts to initialise state and optionally produce messages,
- at least, a function `onMessage()`, aimed to transform one input message into zero, one or more output messages,
- possibly, a function `onInterval()`, called at regular intervals to produce aggregated messages,

```ts
interface FlowStep {
  // called once when the flow (or the step) starts or when a step is reloaded
  // it can be used to initialise state and optionally produce messages
  onStartup(time: Date, context: Context): null | Message | Message[],

  // transform one input message into zero, one or more output messages
  onMessage(message: Message, context: Context): null | Message | Message[],
  
  // called at regular intervals to produce aggregated messages
  onInterval(time: Date, context: Context): null | Message | Message[]
}
```

A message has three attributes: a `topic`, a `payload` and a processing timestamp.

```ts
type Message = {
  topic: string,
  payload: Uint8Array,
  time: Date
}
```

:::note
The message `payload` is an array of unsigned bytes that has to be explicitly converted to a string when appropriate:

```js
const utf8 = new TextDecoder();

export function onMessage(message) {
    let string_payload = utf8.decode(message.payload)
    let json_payload = JSON.parse(string_payload)
    // ..
}
```
:::

A message can also contain protocol specific metadata (currently, only MQTT related metadata).
These metadata are set when the message is received from MQTT (flow input).
and used by the flow when the message is to be published over MQTT (flow output).

```ts
type Message = {
  topic: string,
  payload: Uint8Array,
  time: Date,
  mqtt?: MqttInfo
}

type MqttInfo = {
  qos?: 0 | 1 | 2,  // default is 1
  retain?: boolean  // default is false
}
```

### Context

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
  config: unknown,
}

type KVStore = {
  // List the keys for which this store holds a value  
  keys(): string[]
  
  // Get the value attached to a key (returning null, if none)
  get(key: string): unknown,
  
  // Set the value attached to a key (removing the key if the provided value is null)
  set(key: string, value: unknown),
  
  // Remove any value attache to a key
  remove(key: string),
}
```

The `context.config` is an object freely defined by the step module, to provide default values such as thresholds, durations or units.

### Callbacks

The `onMessage` function is called for each message to be transformed
  - The arguments passed to the function are:
    - The message `{ topic: string, payload: Uint8Array, time: Date }`
    - A context object with the config and state
  - The function is expected to return zero, one or many transformed messages `[{ topic: string, payload: Uint8Array | string }]`
  - An exception can be thrown if the input message cannot be transformed.

A flow script can also export a `onInterval` function
  - This function is called at a regular pace with the current time and context.
  - The flow script can then return zero, one or many transformed messages
  - By sharing an internal state between the `onMessage` and `onInterval` functions,
    the flow script can implement aggregations over a time window.
    When messages are received they are pushed by the `onMessage` function into that state
    and the final outcome is extracted by the `onInterval` function at the end of the time window.

A flow script can also export a `onStartup` function
  - The `onStartup(time, context)` callback is invoked once when a flow is started and
    also when an individual step module is reloaded while the mapper is running.
  - Typical uses are: initialise `context` state, load or compute reference data, and
    optionally produce one-time messages that should be emitted at startup.
  - When a flow is initially loaded, `onStartup` callbacks for the involved steps are
    invoked in the flow activation sequence. When a single step file is modified and
    reloaded, the `onStartup` for that step is invoked (reload behaviour depends on the
    mapper runtime and flow layout).
  - Messages returned by `onStartup` are treated as regular output from the step, passed down to the
    subsequent `onMessage` steps of the flow, and are emitted according to the flow's output
    configuration (they are subject to the same loop-detection and output filtering safeguards as
    other messages).

    Example (simplified):

    ```js
    export function onStartup(_time, context) {
      // initialize a shared value in the flow-level context
      context.flow.set("units", JSON.stringify({ temp: "C" }))
      // optionally return an initial message to be published by the flow
      return { topic: "my/init", payload: "startup-complete" }
    }

    export function onMessage(message, context) {
      // normal per-message processing
    }
    ```

    For a more real-world example of how `onStartup` can be used, see
    [`thingsboard-registration`](https://github.com/thin-edge/tedge-flows-examples/blob/10b4ac9560dde74f079efb3c9a46ea1167a0ded5/flows/thingsboard-registration/src/main.ts#L41-L48)
    example.

## Flow configuration

- The generic mapper loads flows and steps stored in `/etc/tedge/mappers/local/flows`.
- A flow is defined by a TOML file with `.toml` extension.
- A step is defined by a JavaScript file with an `.mjs` or `.js` extension.
  - This can also be a TypeScript module with a `.ts` extension.
- The definition of flow defines its input, output and error sink as well as a list of transformation steps.
- Each step is built either from a `script` or a `builtin` transformation
- A step can possibly be given a config (an arbitrary json object that will be passed to the transformation script)
- Configuration values can also be defined at the flow level,
  these values will be used as default configuration values by all the steps.
- The pace at which a step `onInterval` function is called defaults to one second,
  and can be configured `{ script = "my-script.js", interval = "60s", config = { .. } }`.

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+"]

config = { format = "rfc3339" }

steps = [
    { builtin = "add-timestamp" },
    { script = "drop_stragglers.js", config = { max_delay = 60 } },
    { script = "te_to_c8y.js" }
]
```

### Parameters

The `params.toml` is an optional file, that can be created to customize specific aspects of the deployed flows
without modifying the original flow definitions or JavaScript code.

The `params.toml` file provides a list of named values, possibly compound. As an example:

```toml
debug = false

[time]
format = "unix"
reformat = true

[interval]
hourly = "3600s"
daily = "24h"
```

These values can then be used as parameters by all the flows which definition `.toml` file sites in the same directory.
The parameter values are injected in flow and step configuration using template expression such as `${params.debug}`
or `${params.time.format}`.

Below shows a simplistic example of the parameterization of a flow:

```toml
name = "foo"
version = "1.0.0"

input.mqtt.topics = ["foo"]

[[steps]]
script = "main.js"
config = { debug = "${params.debug}", format = "${params.time.format}" }
interval = "${params.hourly}"
```

:::note
The intent as a `params.toml` file is to ease flow packaging.
Users can adapt flows to their specific use-cases without changing flow definitions and scripts
as provided by the flow authors, provided the flow steps are configured after parameters.  

When a flow use parameters, the advice is to provide an example file called `params.toml.template`
which contains the list of parameterizable values that the user can set.
The parameters of a flow are then derived as a combination of `params.toml.template` (the default values)
and `params.toml` (the user-provided values).

To parameterize a flow, the user should copy the `params.toml.template` file to the `params.toml` location,
and then customize any of the values inside the file.
Ideally the `params.toml.template` includes some documentation about each parameter to assist the user.
:::

Parameter values can be used to parameterize the following parts of a flow:

- Flow input
  - `input.mqtt.topics`
  - `input.process.topic`
  - `input.process.command` 
  - `input.process.interval` 
  - `input.file.topic`
  - `input.file.path` 
  - `input.file.interval`
- Flow config
  - `config.*`
- Steps
  - `steps[].config.*`
  - `steps[].interval`
- Flow output
  - `output.mqtt.topic` 
  - `output.file.path` 

:::note
Substitution rules differ slightly when applied to `config` objects compared to topics, commands, paths and intervals.

For flows and steps config objects, a substitution applies to a given config property,
as in `config = { x = ${params.some.parameter}, y = ${params.another.parameter}  }`
and the substituted values can be arbitrary values (strings, numbers, booleans, arrays or objects).

For topics, commands, paths and intervals, several substitutions can be applied to build a string
(a topic, command, path or interval definition) from several independent parameters,
as in `input.mqtt.topics = [ "te/device/${params.child}/service/${params.service}/m/${params.measurement.type}" ]`
:::

### Transformation

The transformation applied by a step is defined either by
- a user-provided `script` implemented in JavaScript
- a builtin transformation provided by %%te%%.

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

## %%te%% flow mapper

The extensible mapper is launched as a regular mapper:

```shell
tedge-mapper local
```

This mapper:

- loads all the flows defined in `/etc/tedge/mappers/local/flows`
- reloads any flow or script that is created, updated or deleted while the mapper is running
- subscribes to each flow `input.mqtt.topics`, dispatching the messages to the `onMessage` functions
- triggers at the configured pace the `onInterval` functions
- publishes memory usage statistics
- publishes flows and steps usage statistics

### Statistics

The memory and cpu statistics published by a mapper for its mapping rules, flows and steps are published over MQTT,
on a sub-topic of the mapper service topic.


```mermaid
graph LR
  root --/--> mapper["mapper topic id"] --/--> channel

  subgraph root
    te
  end

  subgraph mapper
    direction LR
    device --/--- device_id["&lt;device_id&gt;"] --/--- service --/--- mapper_id["&lt;mapper_id&gt;"]

    style device_id stroke-width:1px
    style mapper_id stroke-width:1px
  end

  subgraph channel
    direction LR
    status --/--- metrics --/--- flow["&lt;flow or script&gt;"]
  end
```

For example, for the Cumulocity mapper:
```
te/device/main/service/tedge-mapper-c8y/status/metrics/measurements.toml
```

Statistics publishing is controlled by the following `tedge config` settings:

- `flows.stats.interval` The interval between statistics dumps (1 hour is the default)
- `flows.stats.on_message` Enable or disable statistics for step onMessage (false is the default)
- `flows.stats.on_interval`  Enable or disable statistics for step onInterval (false is the default).

## %%te%% flow cli

Flows and steps can be tested using the `tedge flows test` command.
- These tests are done without any interaction with MQTT and `tedge-mapper local`,
  meaning that tests can safely be run on a device in production
- By default, tests run against the flows and scripts used by `tedge-mapper local`.
  - However, a directory of flows under development can be provided using the `--flows-dir <FLOWS_DIR>` option.
  - One can also test flows of a builtin mapper using the `--mapper` and `--profile` options.
    The tests will then run against the flows of that mapper (and profile if any).
  - The subcommand `tedge flows config-dir` can be used to get the flows directory for a `--mapper` and a `--profile`.
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
- [`crypto.randomUUID`](https://developer.mozilla.org/en-US/docs/Web/API/Crypto/randomUUID)
  - Generate v4 UUIDs
- [`crypto.getRandomValues`](https://developer.mozilla.org/en-US/docs/Web/API/Crypto/getRandomValues)
  - Get cryptographically strong random values
  - Supported: `Int8Array`, `Uint8Array`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`
  - Not supported: `Uint8ClampedArray`, `BigInt64Array`, `BigUint64Array` 

## Builtin transformations

### `add-timestamp`

Add a timestamp to JSON messages
- The `format` to be used is either `unix` (the default) or `rfc3339`.
- Unless specified otherwise the name for the added timestamp `property` is `time`.
- When the input message already has a timestamp property the default is to let it unchanged.
  This can be changed with the `reformat` config so any timestamp is reformated to the requested format. 
- `{ builtin = "add-timestamp", config = { format = "rfc3339", reformat = true }}`

### `ignore-topics`

Filter out messages with specific topics
- Must be configured with a list of `topics` and topic filters to be ignored
- `{ builtin = "ignore-topics", config.topics = ["te/device/main/service/mosquitto-c8y-bridge/#"] }`

### `limit-payload-size`

Filter out messages which payload is too large
- Must be configured with the `max_size` for the messages (maximum number of bytes)
- Can be configured to `discard` the messages instead of raising an error (the latter being the default)
- `{ builtin = "limit-payload-size", config = { max_size = 64000, discard = true }}`

### `set-topic`

Assign a target topic to messages
- Must be given the `topic` the messages have to be sent to.
- `{ builtin = "set-topic", config.topic = "c8y/measurement/measurements/create" }`

### `update-context`

Store a message in the [mapper context](#context) shared by all the flows and transformation steps.
- The message topic is used as the key and the message payload as the shared value.
  - The message payload is stored as is. It has to be a valid JSON payload, though
  - If the message payload is the empty string, then any previously stored value is removed from the context.
  - A transformation step can then get the value associated to a key from the mapper context
    - `const value = context.mapper.get(key)`
- An `update-context` step can be given a topic filter used to store only a subset of the messages.
  - `{ builtin = "update-context", config.topics = "te/+/+/+/+/m/+/meta" }`
  - If a message doesn't match the configured topic, this message is passed unchanged to the subsequent transformation steps.

### `into-c8y-measurements`

Transform a [%%te%% measurement](../../../understand/thin-edge-json/#measurements) into a [Cumulocity measurement](../c8y-mapper/#measurement)

- This transformer uses the [mapper context](#context) to retrieve metadata about the message sources.
  - In the case of the Cumulocity mapper, this context is populated by the mapper.
  - However, in the case of a user-defined mapper, this context has to populated by a flow consuming MQTT messages
    or reading a configuration file. This is done by inserting [registration messages](../mqtt-api.md#entity-registration)
    using the [entity topic identifiers](../mqtt-api.md#group-identifier) as keys.
  - ```
    context.mapper.set("device/child-xyz//", {
        "@id":"raspberry-007-child-xyz",
        "@type":"child-device",
        "@topic-id":"device/child-xyz//",
        "@name":"child-xyz",
        "@type-name":"thin-edge"
     })
    ```
- If the source that is sending alarms is not registered yet in the mapper context,
  then this transformer discards the alarm. If this is not the desired behavior an approach is to add upstream a step
  using the [`cache-early-messages`](#cache-early-messages) transformer.
- Even if designed after Cumulocity requirements, this transformer can be used by any mappers.

### `into-c8y-events`

Transform a [%%te%% event](../../../understand/thin-edge-json/#events) into a [Cumulocity event](../c8y-mapper/#events)

- This transformer uses the [mapper context](#context) to retrieve metadata about the message sources.
  - In the case of the Cumulocity mapper, this context is populated by the mapper.
  - However, in the case of a user-defined mapper, this context has to populated by a flow consuming MQTT messages
    or reading a configuration file. This is done by inserting [registration messages](../mqtt-api.md#entity-registration)
    using the [entity topic identifiers](../mqtt-api.md#group-identifier) as keys.
  - ```
    context.mapper.set("device/child-xyz//", {
        "@id":"raspberry-007-child-xyz",
        "@type":"child-device",
        "@topic-id":"device/child-xyz//",
        "@name":"child-xyz",
        "@type-name":"thin-edge"
     })
    ```
- If the source that is sending alarms is not registered yet in the mapper context,
  then this transformer discards the alarm. If this is not the desired behavior an approach is to add upstream a step
  using the [`cache-early-messages`](#cache-early-messages) transformer.
- Even if designed after Cumulocity requirements, this transformer can be used by any mappers.

### `into-c8y-alarms`

Transform a [%%te%% alarm](../../../understand/thin-edge-json/#alarms) into a [Cumulocity alarm](../c8y-mapper/#alarms)

- This transformer uses the [mapper context](#context) to retrieve metadata about the message sources.
  - In the case of the Cumulocity mapper, this context is populated by the mapper.
  - However, in the case of a user-defined mapper, this context has to populated by a flow consuming MQTT messages
    or reading a configuration file. This is done by inserting [registration messages](../mqtt-api.md#entity-registration)
    using the [entity topic identifiers](../mqtt-api.md#group-identifier) as keys.
  - ```
    context.mapper.set("device/child-xyz//", {
        "@id":"raspberry-007-child-xyz",
        "@type":"child-device",
        "@topic-id":"device/child-xyz//",
        "@name":"child-xyz",
        "@type-name":"thin-edge"
     })
    ```
- If the source that is sending alarms is not registered yet in the mapper context,
  then this transformer discards the alarm. If this is not the desired behavior an approach is to add upstream a step
  using the [`cache-early-messages`](#cache-early-messages) transformer.
- Even if designed after Cumulocity requirements, this transformer can be used by any mappers.

### `cache-early-messages`

Cache all messages for an entity (the main device, a child device or a service),
till that entity is registered, and its metadata is stored in the mapper context.

The typical usage is to use a two-step flow, where `cache-early-messages` is used
to postpone message transformations till the second step is ready to process them.
Any message that is received for an entity that is not registered yet is cached
and only released when the source entity is properly registered and its metadata stored in flows `context.mapper`.

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+"]

config = { topic_root = "te" }

# Cache measurements till source metadata are stored in the context
[[steps]]
builtin = "cache-early-messages"

# Process measurements assuming source metadata are available
[[steps]]
builtin = "into-c8y-measurements"
```
