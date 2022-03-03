# Summary

The `tedge_api` crate brings centralized definitions of both messages and
behaviour by introducing traits for runtime Plugins to hook into. One goal is
to be *agnostic* on whether a given `Plugin` is part of the same executable or
not. As all messages are easily de/serializable they are also transport
agnostic, allowing implementers and users to de/construct them from/for whatever
sources/destinations they see fit.

# Motivation

Users and developers need to have a common ground to future proof communication
and growth. A common crate to both will make sure that all sides do not grow
apart and stay compatible to each other.

# Guide-level explanation

_This section is meant to be read as if the feature was_ already _implemented._

-----

ThinEdge is an edge focused IoT framework. At its core it serves to bridge the
gap between devices and the cloud. As it is meant to run on low-resources
devices it's core architecture supports that.
As such, it is built upon a message passing router with a modular approach for
extending its functionality through plugins.

ThinEdge can thus be understood as being a common-core of a message passing router
with a collection of plugins that ultimately define what it actually _does_.

Plugins come in two forms:

- Run-time & external, which are provided to Thin-Edge and meant to
  interoperate with it specifically through stdin/stdout or some other
  well-specified interface (cf. external plugins further down)
    - These are for example custom built executables that a user provides and
      wishes to not integrate into ThinEdge for various reasons (e.g. language
      differences or license)
- Compile-time & built-in, which are compiled into the actual ThinEdge binary
  and written in Rust.
    - These offer the advantage of simplifying deployment as well as assurances
      w.r.t. the messages (i.e. changes)

Both ways are _functionally equivalent_ and _support the same features_.

We aim to provide plugins for all common server and use-cases, see further down
if you wish to extend Thin-Edge yourself.

## Configuring your plugins

ThinEdge is on its own merely a collection of plugin kinds, and requires a
configuration to do any useful work. A common configuration includes a cloud
mapper, some device management plugins as well as some data sources.

> One core idea is that you can create as many plugin instances of a single
> plugin _kind_ as you wish. For example you could have multiple "File Watcher"
> instances that each watch a different file. 


Here is an fictuous configuration that would connect the device to the acme cloud.

```toml
[plugins.acme]
kind = "acme_mapper"
[plugins.acme.configuration]
tenant = "coyote"
software_mgmt_plugin = "pacman" # Here we tell the acme_mapper plugin where to
                                # send software management requests
service_mgmt_plugin = "systemd"

[plugins.pacman]
kind = "pacman_handler"
[plugins.pacman.configuration]
allow_destructive_operations = true # Per default the plugin does not allow
                                    # adding/removing packages

[plugins.systemd]
kind = "systemd_handler"
configuration = { 
    restrict_units_to = ["nginx.service"]  # Only allow interacting with the nginx service
}
```

Of note here is that everying in the `plugins.<plugin id>.configuration` space
is per plugin! Each plugin exposes its own set of configurations depending on
its needs and abilities. Nonetheless some parts are probably more common:

- Per default plugins should strive to do the 'safe' thing. Ambiguitiy should
  be reduced as much as possible and if defaults are unclear should either
  force the user to specify it or do an idempotent safe/pure operation. (e.g.
  the pacman plugin only allows listing packages per default)
- Plugins don't guess where to send their messages to. If the `acme_mapper`
  receives a 'Restart "nginx" service' message it needs to be configured to
  tell it where to find the destination for it.

Once you have this configuration file, you can go on and start Thin-Edge

## Starting Thin-Edge

On startup `tedge` will:

- Check if your configuration is syntactically correct and that all requested
  kinds exist
- Startup the requested plugins and process messages

In this case, the `heartbeat` service will keep sending messages every 400ms to
both `watch_sudo_calls` and `check_service_alive` with a
`MessageKind::SignalPluginState` to which they should answer with their
status, e.g. `PluginStatus::Ok`.

## Writing your own plugins

### For External Plugins

External plugins are executables that interact with the `tedge` api through a
specific compile-time plugin, like for example `StdIoExternalPlugin` which
communicates through STDIN/STDOUT.

To use it, simply choose the plugin that fits your communication style and move
to the configuration section below.

### For Compile Plugins

Compile-time plugins are written in Rust. They require two parts: A
`PluginBuilder` and a `Plugin`. 

- The `PluginBuilder` creates instances of `Plugins` and verifies if the
  configuration is sane.
- The `Plugin` is the object that contains the 'business logic' of your plugin

So, to get started:

1. Implement `PluginBuilder` for the struct that you will use to instantiate your plugin.

It looks like this:

```rust
/// A plugin builder for a given plugin
#[async_trait]
pub trait PluginBuilder: Sync + Send + 'static {
    /// The a name for the kind of plugins this creates, this should be unique and will prevent startup otherwise
    fn kind_name(&self) -> &'static str;

    /// This may be called anytime to verify whether a plugin could be instantiated with the
    /// passed configuration.
    async fn verify_configuration(&self, config: &PluginConfiguration) -> Result<(), PluginError>;

    /// Instantiate a new instance of this plugin using the given configuration
    ///
    /// This _must not_ block
    async fn instantiate(
        &self,
        config: PluginConfiguration,
        tedge_comms: Comms,
    ) -> Result<Box<dyn Plugin + 'static>, PluginError>;
}
```

Things of note here:

- `kind_name` _has_ to be unique, and will be used to refer to this kind of plugin
- `verify_configuration` allows one to check if a given configuration _could even work_
    - It however is not required to prove it
- `instantiate` which actually constructs your plugin and returns a new instance of it
    - One argument is the `Comms` object which holds the sender part of a
      channel to the tedge core, and through which messages can be sent


2. Implement `Plugin` for your plugin struct.

`Plugin` is defined as follows:

```rust
/// A functionality extension to ThinEdge
#[async_trait]
pub trait Plugin: Sync + Send {
    /// The plugin can set itself up here
    async fn setup(&mut self) -> Result<(), PluginError>;

    /// Handle a message specific to this plugin
    async fn handle_message(&self, message: Message) -> Result<(), PluginError>;

    /// Gracefully handle shutdown
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

Plugins follow a straightforward lifecycle:

- After being instantiated the plugin will have a chance to set itself up and get ready to accept messages
- It will then continuously receive messages as they come in
- If it ever needs to be shutdown, its shutdown method will give it the opportunity to do so.

See `message::Message` for possible values.

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

To make sure that the program itself is _hard to misuse_, the configuration is
read _once_ at startup.

-------

# Reference Explanation

The core design part of ThinEdge software are messages that get passed around.
This has these benefits:

- Simple architecture as 'dumb plumbing' -> complexity is handled by the
  plugins
- Clean separation of different parts, using messages codifies what can be
  exchanged and makes it explicit

An example communication flow (with an arrow being a message, except 1 and 10):

```

 ┌─────────────┐
 │ MQTT Broker │
 └┬───▲────────┘
  │1  │10    azure_mqtt
 ┌▼───┴────────────────┐
 │MQTT Plugin          │
 │                     │
 │Target: azure_cloud  │
 │                     │
 │Data: Proprietary    │
 │                     │                azure_cloud
 └───────┬─────▲───────┘               ┌──────────────────────────────┐
         │     │               3       │Azure Plugin                  │
         │     │9            ┌─────────►                              │
         │     │             │         │Target: service_health        │
        2│   ┌─┴─────────────┴─┐    4  │                              │
         └───►                 ◄───────┤Data: GetInfo "crit_service"  │
             │                 │       ├──────────────────────────────┤
             │      CORE       │  7    │                              │
             │                 ├───────► Target: azure_mqtt           │
             │                 │  8    │                              │
             │                 ◄───────┤ Data: Proprietary            │
             │                 │       └──────────────────────────────┘
             └────────▲───┬────┘
                      │  5│  service_health
                      │   │ ┌──────────────────────┐
                     6│   │ │systemd Plugin        │
                      │   └─►                      │
                      │     │Target: <sender>      │
                      │     │                      │
                      └─────┤Data: OK              │
                            │                      │
                            └──────────────────────┘

```

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

# Unresolved questions

- How to extend the `MessageKind` type?
    - The enum itself is `#[non_exhaustive]`, but extending it still requires a
      whole developer story
    - What is the process of adding new variants?
- How does the IO interface look like for external plugins?
    - Which ones should exist? Just StdIO at first, HTTP maybe later?
- How to delineate between different plugin kinds in terms of messages it should be able to handle?
    - For example are services always 'on system' and if one wants to restart a
      container one needs to be _specific_ about the container? 
        - If yes, how do mappers potentially make the difference?

# Future possibilities

As all messages are routed through the application itself, it should be
possible to add other transformations to messages as they are being handled.

- Logging of what communicated with what
- Access Control
- Overriding destinations
