# Thin-Edge API requirements and design

One should be able to build a thin-edge executable from tedge plugins crates that have been implemented independently:

* All these tedge plugins crates depends on the tedge_api crate that defines how
  to create, connect and run plugin instances.
* Each tedge plugin crate can define its own types of messages. For instance, a
  tedge_telemetry plugin can defines measurements, events, alarms and setpoints
  messages.
* To exchange typed messages, plugin crates don't need a direct dependency relationship.
  Instead, they only need to be aware of the same type. If a plugin can send a
  certain type of message and the other plugin can receive this exact same type,
  these plugins may communicate. Whether the message type is defined within the
  codebase of one of these plugins or some other crate doesn't matter.
* The tedge_api crate defines only messages related to the runtime as Shutdown.
* The implementation of the mechanisms defined by the tedge_api are provided by
  a tedge_runtime crate.
* The final executable is build as an assemblage of all these plugin crates and
  a `main.rs` that defines which plugins have to be instantiated and then
  launched.

---------

# Non-functional Requirements

- Plugins can be included in the final executable without having them 'instantiated'
  - The motivation is to be able to deliver a battery included executable, ready
    to be used for a large set of use-cases without any Rust expertise.
  - Notably a battery-included executable should include an MQTT connection
    plugin and a JSON plugin for telemetry messages.
- Plugins are instantiated and configured through an external configuration at
  runtime
- At startup the configuration is read and from it a set of plugins 'to be
  instantiated' is derived
- One should be able to deduplicate parts of the configuration (to prevent
  repetition)
- Should plugin instances be changeable during runtime? (open question)
- Should some plugin configurations be changeable at runtime or not?
- Having an optional plugin handling SIGHUP and send an internal message
  UpdateConfig to all the running plugin instances.
- Would having multiple configuration files be an option? (open question)
- Having plugins included but not instantiated should not impact other
  instantiated plugins
- A plugin should be able to signal the correctness of its configuration to the
  best of its abilities
- The runtime should not be able to start with a clear broken configuration
- One plugin panicking should not impact other plugins or the runtime
- If a plugin does not support a message that another plugin expects it to, the
  runtime should not start
  - Same thing for a missing required peer for a given kind, or two peers when
    just one is expected.
- Plugin/Application shutdown
    - Shutdown is signalled to all plugins, giving them a possibility to handle
      this case
- A plugin to handle SIGTERM? Or part of the core?
- Plugins should be free to communicate with any other plugins, even in a
  circular fashion.
  - An example: A message is received over MQTT -> Processed & Logged -> Sent
    back over MQTT
- Plugin messages should be able to send back a reply, and potentially multiple

