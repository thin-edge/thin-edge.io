# Summary

Currently there are is no centralized specification of both behaviour and
messages. This crate would bridge this gap by introducing traits for potential
runtimes to hook into. The goal is to be *agnostic* on whether a given `Plugin`
is part of the same executable or not. Since all messages are meant to be
easily de/serializable they are also transport agnostic, allowing implementers
and users to configure it how they see it fit.

# Motivation

Users and developers need to have a common ground to future proof communication
and growth. A common crate to both will make sure that all sides do not grow
apart.

# Guide-level explanation

_This section is meant to be read as if the feature was_ already _implemented._


-----

Thin-Edge is an edge focused IoT framework built upon a message passing core
with a modular approach for extending its functionality.

Thin-Edge can be extended with two flavors of plugins:

- Compile-time, which are built into the binary and written in Rust.
    - These provide the benefit of assuring compatibility with future versions
      during the build step
- External, which are executables that simply exist on your target system

Both are functionally equivalent and support the same features. If you are new
to the project you should first get an overview of the project and its goals.

Creating a new plugin then takes the following steps:

**For Compile Plugins**

1. Implement `PluginBuilder` for the struct that you will use to instantiate your plugin.

It looks like this:

```rust
/// A plugin builder for a given plugin
pub trait PluginBuilder: Sync + Send + 'static {
    /// The name of the plugins this creates, this should be unique and will prevent startup otherwise
    fn name(&self) -> &'static str;

    /// This may be called anytime to verify whether a plugin could be instantiated with the
    /// passed configuration.
    fn verify_configuration(&self, config: &PluginConfiguration) -> Result<(), PluginError>;

    /// Instantiate a new instance of this plugin using the given configuration
    ///
    /// This _must not_ block
    fn instantiate(
        &self,
        config: PluginConfiguration,
        tedge_comms: Comms,
    ) -> Result<Box<dyn Plugin + 'static>, PluginError>;
}
```

Things of note here:

- `name` _has_ to be unique, and will be used to refer to this kind of plugin
- `verify_configuration` allows one to check if a given configuration _could even work_
    - It however is not required to prove it
- `instantiate` which actually constructs your plugin and returns a new instance of it
    - One argument is the `Comms` object which holds the sender part of a
      channel to the tedge core, and through which messages can be sent



2. Implement `Plugin` for your plugin struct.

`Plugin` is defined as follows:

```rust
#[async_trait]
pub trait Plugin: Sync + Send {
    /// The plugin can set itself up here
    async fn setup(&mut self) -> Result<(), PluginError>;

    /// Handle a message specific to this plugin
    async fn handle_message(&self, message: PluginMessage) -> Result<(), PluginError>;

    /// Gracefully handle shutdown
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

It follows a straightforward lifecycle:

- After being instantiated the plugin will have a chance to set itself up and get ready accept messages
- It will then continuously receive messages as they come in (and are addressed to it)
- If it ever needs to be shutdown, its shutdown method will give it the opportunity to do so.

See `PluginMessage` for possible values.


**For External Plugins**

External plugins are executables that interact with the `tedge` api through a
specific compile-time plugin, like for example `StdIoExternalPlugin` which
communicates through STDIN/STDOUT.

To use it, simply choose the plugin that fits your communication style and move
to the configuration section below.


**Configuring your plugin**

Only adding a compile time plugin to the list of plugins does nothing on its
own. They simple serve as 'factories' of the given plugin, depending on how you configure it.

An example configuration can be seen below. It configures a heartbeat plugin as
well as an external bash script that replies to the heartbeats.

The configuration:

```toml
[plugins.simple-heartbeat] # 'simple-heartbeat' is the name of this _instance_,
                           # it may be the same as its kind, both are unique to
                           # each other though
kind = "heartbeat" # 'heartbeat' is the compile-time name of the plugin

configuration = { targets = ["simple-bash", "simple-bash2"] }

[plugins.simple-bash]
kind = "stdio-external"

configuration = { path = "/usr/bin/bash-service", interval-ms = 500 }

[plugins.simple-bash2] # There can be more than one instance of the same kind of plugin
kind = "stdio-external"

configuration = { path = "/usr/bin/bash-service", interval-ms = 500 }
```

The bash file:

```bash
# Rethink hard if you actually want to implement this in bash
# TODO
```

**Starting tedge**

On startup `tedge` will:

- Check if your configuration is syntactically correct and that all requested
  kinds exist
- Startup the requested plugins and process messages

In this case, the `heartbeat` service will keep sending messages every 500ms to
both "simple-bash" and "simple-bash2" with a `PluginMessage::SignalPluginState`
to which they should answer with their status, e.g. `PluginStatus::Ok`.

-------

At the heart of these choices lies the idea of making sure that using ThinEdge
is precise, simple, and hard to misuse.

- In the above example, the `heartbeat` service kind would check if the targets
  are actually existing plugins it could check the heartbeat on _before_ the
  application itself would start!

- Similarly, the `stdio-external` plugin kind would check that the file it is
  given as `path` is accessible and executable!

Users should not be mislead by too simple error messages, and in general be
helped to achieve what they need. Warnings should be emitted for potential
issues and errors for clear misconfigurations.

Similarly, _using_ the different plugins should follow the same idea, with
clear paths forwards for all errors/warnings, if possible.

----

# Reference Explanation

The core design part of ThinEdge software are messages that get passed around.

## Messages 

Each message carries its source and destination, an id, and a payload. The
payload is _well defined_ in the `MessagePayload` enum. This means that the
amount of different message kinds is limited and well known per-version. The
message kinds are forward compatible. Meaning that new kinds of messages may be
received but simply rejected if unknown.

Messages conversations are also asynchronous, meaning that upon receiving a
message ThinEdge might not reply. (This does not preclude the transport layer
to assure that the message has been well received)

This has two reasons:

- It makes communication easier to implement
- It reflects the network situation

Using such an interface is still clunky without some additional help.

As all messages have an ID, it could be possible to design a way of 'awaiting'
a response. This is left as a future addition.

## Plugins

_Note: 'Plugins' is simply a working name for now. We are free to rename this
to 'Extension' or any other name we think is good._

Plugins are what makes ThinEdge work! They receive messages and execute their
specific action if applicable.

This part is split into two parts: PluginBuilders and Plugins themselves.

### PluginBuilders

A `PluginBuilder` is a Rust struct that implements `PluginBuilder`. They are the
sole sources of plugin kinds in ThinEdge. (This is not a limitation for
proprietary and non-rust plugins, read further to see how those are handled.)
They are registered at startup and hardcoded into the ThinEdge binary.

### Plugins

A Plugin is a Rust struct that implements `Plugin`. It is clear that requiring
all users who wish to extend ThinEdge to learn and use Rust is a _bad idea_. To
make sure those users also get a first class experience special kinds of
plugins are bundled per default: `stdio-external`, `http-external`. (The
specific list will surely evolve over time.) These get instantiated like any
other with the exception that it is expected that all messages are forwarded,
e.g. to another binary or http endpoint. This way any user simply needs to know
what kind of messages exists and how to send/receive them, giving them the same
situation like a pure-Rust plugin would have.

# Drawbacks

- Tying the software specification to Rust code makes it more brittle to
  accidental incompatibilities
- People might get confused as to how Plugins can be written -> They see "Rust"
  and think they need to know the language to make their own
- Having everything in a single process group can make it harder to correctly
  segment security rules for individual plugins

# Rationale and alternatives

Playing to Rust's strengths makes for better maintainable software. Using
Traits to specify behaviour and "plain old Rust objects" also make the messages
clear. As ThinEdge is a specialized piece of software, this should be embraced
to deliver on an amazing experience.

Alternatives could include:

- Defining the objects externally, with for example CapNProto
- Defining the objects through an API like Swagger or similar
- Not doing this

# Prior Art

# Unresolved questions

# Future possibilities
