# Extensible mapper and user-provided flows

* Date: __2025-07-1-__
* Status: __New__

## Motivation

In theory, %%te%% users can implement customized mappers to transform data published on the MQTT bus
or to interact with the cloud. In practice, they don't.
Implementing a mapper is costly while what is provided out-the-box by %%te%% already meets most requirements.
The need is not to write new mappers but to adapt existing ones.

The aim of the extensible mapper it to let users extend and adapt the mappers with their own filtering and mapping rules,
leveraging the core mapping rules and mapper mechanisms (bridge connections, HTTP proxies, operations).

## Vision

The %%te%% mappers for Cumulocity, Azure, AWS and Collectd are implemented on top of an extensible mapper
which is used to drive all MQTT message transformations.
- Transformations are implemented by flows which consume MQTT messages, apply a sequence of transformation steps and produce MQTT messages.
  - `MQTT sub| step-1 | step-2 | ... | step-n | MQTT pub`
- A flow can combine builtin and user-provided steps.
- The user can configure all the transformations used by a mapper,
  editing MQTT sources, flows, steps and MQTT sinks.
- By contrast with the current implementation, where the translation of measurements from %%te%% JSON to Cumulocity JSON
  is fully hard-coded, with the generic mapper a user can re-use the core of this transformation while adding customized steps:
  - consuming measurement from a non-standard topic
  - filtering out part of the measurements
  - normalizing units
  - adding units read from some config
  - producing transformed measurements on a non-standard topic.

## POC reference

- The generic mapper loads flows and steps stored in `/etc/tedge/flows/`.
- A flow is defined by a TOML file with `.toml` extension.
- A step is defined by a Javascript file with `.js` extension.
- The definition of flows must provide a list of MQTT topics to subscribe to.
  - The flow will be feed with all the messages received on these topics.
- A flow definition provides a list of steps.
  - Each step is built from a javascript and is possibly given a config (arbitrary json that will be passed to the script)
  - Each step can also subscribe to a list of MQTT topics (which messages will be passed to the script to update its config)

```toml
input.mqtt.topics = ["te/+/+/+/+/m/+"]

steps = [
    { script = "add_timestamp.js" },
    { script = "drop_stragglers.js", config = { max_delay = 60 } },
    { script = "te_to_c8y.js", meta_topics = ["te/+/+/+/+/m/+/meta"] }
]
```

- A flow script has to export at least one `process` function.
  - `process(t: Timestamp, msg: Message, config: Json) -> Vec<Message>` 
  - This function is called for each message to be transformed
  - The arguments passed to the function are:
    - The current time as `{ seconds: u64, nanoseconds: u32 }` 
    - The message `{ topic: string, payload: string }`
    - The config as read from the flow config or updated by the script
  - The function is expected to return zero, one or many transformed messages `[{ topic: string, payload: string }]`
  - An exception can be thrown if the input message cannot be transformed.
- A flow script can also export an `update_config` function
  - This function is called on each message received on the `meta_topics` as defined in the config.
  - The arguments are:
    - The message to be interpreted as a config update `{ topic: string, payload: string }`
    - The current config
   - The returned value (an arbitrary JSON value) is then used as the new config for the flow script.
- A flow script can also export a `tick` function
  - This function is called at a regular pace with the current time and config.
  - The flow script can then return zero, one or many transformed messages
  - By sharing an internal state between the `process` and `tick` functions,
    the flow script can implement aggregations over a time window.
    When messages are received they are pushed by the `process` function into that state
    and the final outcome is extracted by the `tick` function at the end of the time window.

## First release

While the POC provides a generic mapper that is fully independent of the legacy mappers,
the plan is not to abandon the latter in favor of the former
but to revisit the legacy mappers to include the ability for users to add their own mapping rules.

To be lovable, the first release of an extensible mapper should at least:

- be a drop-in replacement of the current mapper (for c8y, aws, az or collect)
- feature the ability to customize MEA processing by combining builtin flow steps with user-provided functions written in JavaScript
- provide tools to create, test, monitor and debug steps and flows
- be stable enough that user-defined flow scripts will still work without changes with future releases.

To keep things simple for the first release, the following questions are deferred:

- Could a generic mapper let users define bridge rules as well as message transformation flows?
- Does it make sense to run such a mapper on child-devices?
- Could a flow send HTTP messages? Or could a flow step tell the runtime to send messages over HTTP?
- How to handle binary payloads on the MQTT bus? 
- Could operations be managed is a similar way with user-provided functions to transform commands?
- To handle operations, would the plugins be expanded to do more complex things like HTTP calls, file-system interactions, etc.? 
- What are the pros and cons to persist flow step states?
- Split a flow, forwarding transformed messages to different flows for further processing

### API

The POC expects the flow scripts to implement a bunch of functions. This gives a quite expressive interface
(filtering, mapping, splitting, dynamic configuration, aggregation over time windows), but at the cost of some complexity.

- `process(t: Timestamp, msg: Message, config: Json) -> Vec<Message>`
- `tick(t: Timestamp) -> Vec<Message>`
- `update_config(msg: Message, config: Json) -> Json`

An alternative is to let the user implement more specific functions with simpler type signatures:

- `filter(msg: Message, config: Json) -> bool`
- `map(msg: Message, config: Json) -> Message`
- `filter_map(msg: Message, config: Json) -> Option<Message>`
- `flat_map(msg: Message, config: Json) -> Vec<Message>`

One can also rearrange the argument order for these functions,
making life easier when a transformation does need a config or the current time
leveraging that one can pass more arguments than declared to a javascript function:

- `process(msg: Message, config: Json, t: Timestamp) -> Vec<Message>`
- `process(msg: Message, config: Json) -> Vec<Message>`
- `process(msg: Message) -> Vec<Message>`

One can even use a bit further the flexibility of javascript, to let the process function freely return:
- An array of message objects
- A single message object
- A null value interpreted as no messages
- A boolean

Other ideas to explore to make the API more flexible:

- Interaction with the entity store and tedge config.
- Allow a flow to subscribe to topics related to the device/entity it is running on
- Feed flow scripts with message excerpts as done for the workflows

### Devops tools

The flexibility to customize MQTT message processing with user-provided functions comes with risks:
- a step might not behave as expected,
- flows might be overlapping or conflicting, possibly sending duplicate messages or creating infinite loops
- builtin flows might be accidentally disconnected or broken
- a step might introduce a performance bottleneck.

To help mitigating these risks, the `tedge mapping` sub-commands provide the tools to test, monitor and debug steps and flows.

- `tedge mapping list [topic]` displays flows and steps messages received on this topic will flow through
  - can be used with a set of flows not configured yet for a mapper
- `tedge mapping test [flow]` feeds a step or flow with input messages and produces the transformed output messages
  - allow users to run an assertion based on the input/output of a flow
  - ability to pipe `tedge mqtt sub` and `tedge mapping test`
  - control of the timestamps
  - test aggregation over ticks
  - can be used with a set of flows not configured yet for a mapper
- `tedge mapping stats [flow]` returns statistics on the messages processed by a flow
  - count message in, message out
  - processing time min, median, max for each flow and step
