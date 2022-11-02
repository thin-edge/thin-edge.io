# Thin-edge actor-based components

The aim of thin-edge is to empower IoT software developers,
with a large set of pre-build components
that can be enabled, configured, extended and combined
into IoT specific agents.

For that purpose, thin-edge leverages the actor model
and provides the tools to combine these actors into independent extensions
and then into full-fledged executables the users can adapt to their use-cases.

This document trace the core decisions toward this design.

## Why actors?

The core idea of the [actor model](https://en.wikipedia.org/wiki/Actor_model) is
to implement a system with independent processing units, the actors, that
* behave accordingly to some internal state to which they have an exclusive access,
* send asynchronous messages to each others,
* process, one after the other, the messages received in their inbox,
* react to messages by sending other messages and updating their state,
* and possibly spawn other actors.

An actor can be:
* a source of messages,
* a mapper that consumes messages and produces translated messages,
* a transducer that consumes and produces messages but also maintains a state ruling its behavior,
* a server that delivers responses on requests,
* an end-point that logs, persists or forwards the messages to some external service.

In a running system, actors are connected with peers along various patterns:

* An actor can gather messages from several sources and dispatch messages to several recipients.
* Messages can be addressed to specific actors, or be broadcast to any interested recipients.
* An actor can be oblivious to the source and the destination of the messages flowing through it,
  processing messages independently of their source,
  and publishing out messages to any interested recipients.
* By contrast, an actor can distinguish its peers,
  possibly processing the messages accordingly to their sources
  and sending specific messages to each of them.
* Notably, a server sends response messages specifically to the requester.

The main benefits of this model in the context of thin-edge are:
* __Modularity__
  * An actor can be understood in isolation.
  * The behavior of an actor is fully determined by its reaction at a given state to some given message.
  * Actors can be implemented, tested, and packaged independently.
* __Flexibility__
  * Actors can be connected in various way,
    as long as the recipients can interpret the messages received from others.
  * An actor can be substituted by another that implements the same service,
    i.e. that sends and receives the same message types.
    These can be different sources of telemetry data
    or different wrappers to software package managers.
* __Observability__
  * The behavior of an actor can be fully observed, by listening the stream of messages received and sent.
  * In a pure actor model setting, any state change can be traced to a message,
    being either triggered by a message or notified by a message.
  * The messages being serializable, they can be logged, persisted, audited, forwarded to the cloud.
* __Testability__
  * An actor can be tested in isolation as well as in combination.
  * An actor can be exercised with arbitrary sequences of input messages,
    while its output is observed and verified,
    and some of its peers being possibly simulated.
  * An actor's peer simulator can be as simple as a pre-registered stream of messages,
    or as sophisticated as an error injector.
  * In a pure actor model setting, all interactions with the system are done via actors,
    and can be simulated, including the clock and the file system.

Thin-edge leverages Rust, adding robustness to the actor model.

* __Robustness__
  * No message can be sent if not understood by the recipient.
    The compatibility, between the messages sent to an actor
    and the messages this actor can actually process,
    is verified at compile-time.

However, using Rust also introduce specific challenges, as we will see below.
To name just a few:

* As all the message types have to be known at compile time,
  we need to be cautious not to make all the actor *implementations* depending on each others,
  due to dependencies at the *message* level.
  * The implementation of an actor that consumes telemetry data
    must not depend on the implementations of actors producing telemetry data - and vice versa.
* As no message can be sent without being understood by the recipient,
  we need a shared definition of IoT messages.
  * Consumers and producers of telemetry data must agree on what is telemetry data.
* However, we also expect the system to be extended by independent vendors,
  and thin-edge should not pre-defined all the messages that can be exchanged by actors.
  * A contributor should be able to define its own set of messages usable by others.  

## From actors to software components

Using actors is key but not sufficient.
Yes, the actor model helps thin-edge rust developers to implement components in isolation.
However, thin-edge also aims to provide flexibility to the agent developers with ready-to-use executables,
that can be tuned on-site for a specific use-case,
by activating only specific features among all those available.

The design must address not only the API for *actors* but also the packaging of these actors into *extensions*,
their integration into *executables* and their *configuration* by agent developers and end-users. 

* Elementary features are provided by __actors__: units of computation that interact with asynchronous messages.
* Actors are packaged into __extensions__: rust libraries that provide implementations along interfaces,
  input and output messages as well as instantiation services.
* A thin-edge __executable__ makes available a collection of extensions ready to be enabled on-demand.
* A user provided __configuration__ enables and configures sherry-picked extensions making an agent.

This document is focused on the implementation of the actor model in the context of thin-edge.
We will address later how a user could sherry-pick extensions from a batteries-included software.

## Requirements

One should be able to build a thin-edge executable from extensions that have been implemented independently.

* The thin-edge API must define how to create, connect and run actor instances.
* Actors should be loosely coupled, only depending on message types defined by peer actors
  and not strongly dependent on specific peer actor implementations.
* The compatibility between two actors, one sending messages the other consuming them,
  must be checked at compile time leveraging rust types for messages and channels.
* If an actor expect to be connected to specific peers, this must be enforced at compile time,
  these peers being solely defined by the types of the exchanged messages. 
* Two actors must not need a direct dependency relationships to be able to exchange messages, requests or responses.
  In practice, one might have crates with sole purpose to define message types.
  So a consumer of messages, don't need to know any sources of such messages.
  And vice-versa, the producers don't have to depend (at the code level) on any consumers.
  Both consumers and producers only need a dependency on the message definitions.
* The thin-edge API should give a large flexibility to connect actors:
  * exchanging stream of messages as for measurements,
  * sending requests which responses are awaited by the requester,
  * sending asynchronous requests which responses will be processed concurrently with the other messages,
  * broadcasting messages,
  * gathering messages from various sources,
  * sending messages to one specific instance of an actor.
* Actors should support being synchronous, with the ability to pause awaiting the response of another actor.
* The final executable is build as an assemblage of extensions, defining actor instances and their connections.
* Using Rust actors must not be the unique way to create MQTT-based thin-edge executables - aka *plugins*.
  An agent developer must be free to choose his preferred programming language
  to implement a feature that interacts with other thin-edge executables using JSON over MQTT.

One should be able to build executables with IIoT specific features.

* The `tedge_actors` crate defines only messages related to the runtime as `Spawn`, `Shutdown` or `Timeout`.
* IIoT related messages are defined in specific extensions as `tedge_telemetry` or `tedge_software_management`.
* An extension must be provided for MQTT as well as another one for HTTP.
* An extension must be provided to encapsulate the thin-edge MQTT API,
  with the definition of all the MQTT topics and message payloads.

Robustness is key.
* One actor panicking should not impact other actors or the runtime.
* All errors must be handled in a non-crashing way.
* Unrecoverable errors may cause the binary to shutdown eventually, but not unexpectedly.
* The framework must handle SIGTERM and signal a shutdown to all active actors.
* Shutdown is signalled to all extensions, giving them a possibility to handle such case gracefully.
  However, the robustness of the solution should not always rely on graceful shutdowns
  and should be designed to cope with unexpected crashes or SIGKILLs.

Observability and testability:
* An actor should be testable in isolation, with
  - a configurable initial state,
  - simulated inputs,
  - checked outputs,
  - possibly simulated peers.
* Every actor must have a unique id built after the actor type as known by the users.
* A user must be able to configure logging per-component as well as globally,
  tracing the messages processed by each actor
  as well as those forwarded to peers.

The runtime itself should behave as an actor, with messages that can be traced.
* Runtime messages should be used for all runtime actions:
  - to activate and deactivate extensions at runtime
  - to start and stop actors,
  - to set and trigger timeout,
  - to shutdown the system.
* Runtime messages sent to an actor must be processed with a higher priority.
* An actor should be able to send messages to the runtime:
  - to trigger an action as spawning a task
  - to notify errors.
* Runtime messages must be traceable as regular messages,
  so a user should be able to observe all runtime actions.

Nice to have ideas that are out-of-scope of the first implementation of the actor model for thin-edge:
* It would be a plus, to have actors storing data using the framework:
  - to persist data between restarts of the deployment
  - to cache data during network outages
  - to provide operation checkpoints during sensitive operations.
* It would be handy to build batteries-included executables
  that contains numerous extensions and alternative implementations for a large diversity of use-cases.
  - An extension can be included in an executable without being enabled, but only registered and ready to be used.
  - Registered extensions can be enabled and configured at runtime. 
  - Having extensions included but not instantiated should not impact other extensions nor consume runtime resources.
  - Such an executable with enabled and non-enabled extensions must provide a command line option, say `--list-extensions`,
    to provide the whole list of available extensions and their purpose;
    as well as command line options for detailed help on how to enable and configure those.

## Proposal

* TODO Why not `actix`?

### Messages

Messages must flow freely between actors with no constraints on ownership and thread.
As they are used to improved observability, they must be `Display`.

```rust
/// A message exchanged between two actors
pub trait Message: Clone + Display + Send + Sync + 'static {}

/// There is no need to tag messages as such
impl<T: Clone + Display + Send + Sync + 'static> Message for T {}
```

Typical examples of thin-edge messages are telemetry data, operation requests and outcomes,
but also low level messages as MQTT messages, HTTP requests and responses,
and even system specific messages as file-system events and update requests.

To be discussed:
* Do the messages need to be `Clone`?
  This has to be considered along the idea of using `oneshot` channel for the response to a request.
  It might be better to be explicit
  and use `Message + Clone` in contexts where messages are broadcast.

### Channels

Multi-producer single-consumer (`mpsc`) channels are used to exchange messages between actors.

* A channel is created for each actor instance.
* The receiver of this channel is given to the recipient actor.
* Clones of the channel sender are given to any actor that needs to send messages to this instance.
* With this setup, each actor instance owns
  - an `mpsc::Receiver` end
  - and a bunch of `mpsc::Sender`s, one per peer. 
* The actors process then in turn the received messages,
  updating their internal state and sending messages to their peers. 

Having a single receiver per actor improves modularity, observability and testability,
since all the inputs for an actor are going through this receiver.
Similarly, having the peers of an actor materialized by channel senders
helps to understand and test the actors in isolation.

However, for this to work, several points need to be addressed, regarding:

* Message types
* Channel types
* Channel creation and ownership
* Actor with no inputs
* Addressing responses
* Out-of-band runtime messages
* Size of the channel buffers

#### Message types

All the messages sent to an actor must have the same rust type - defined by the actor.
So, they can be queued into the actor receiver and then processed in turn.

```rust
pub trait Actor {
    /// Type of input messages this actor consumes
    type Input: Message;
    
    // ...
}
```

However, in practice, an actor has to handle different message kinds.
For instance, a `c8y_mapper` actor handle concurrently:
* telemetry data received from sensors and child devices,
* operation requests coming from the cloud,
* operation outcomes returned by the operating system and child devices.

These different kinds of message have to be encapsulated into a single type, an `enum`.
However, an actor sending messages *must not* depend on this global enum. 
* In the `c8y_mapper` example case:
  It's critical for an actor that just sends telemetry data
  to not depend on the other kinds of messages, as those related to operations.
  Otherwise, we would lose the ability to implement these two actors independently.
* For an actor to send messages to another one,
  one must only ensure that the messages sent
  can be converted *into* those expected by the recipient.

The `fan_in_message_type!` macro helps to define such an `enum` type
grouping subtypes of message that can be sent by independent actors.
The expression `fan_in_message_type!(Msg[Msg1,Msg2]);` expends to:

```rust
#[derive(Clone, Debug)]
enum Msg { 
    Msg1(Msg1),
    Msg2(Msg2),
}

impl From<Msg1> for Msg {
    fn from(m: Msg1) -> Msg {
        Msg::Msg1(m)
    }
}

impl From<Msg2> for Msg {
    fn from(m: Msg2) -> Msg {
        Msg::Msg2(m)
    }
}
```

#### Channel types

#### Channel creation and ownership

#### Actor with no inputs

#### Out-of-band runtime messages

#### Size of the channel buffers

### Behavior

### Instantiation

### Discovery

### Runtime

### Runtime messages

