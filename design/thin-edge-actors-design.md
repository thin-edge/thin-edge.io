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

[There are a *lot* of actor frameworks for Rust](https://www.reddit.com/r/rust/comments/n2cmvd/there_are_a_lot_of_actor_framework_projects_on/),
so why thin-edge should come with its own implementation?

As [well described by Alice Ryhl](https://ryhl.io/blog/actors-with-tokio/),
actors can be implemented with Tokio directly, without using any actor libraries.
Indeed, [tokio](https://docs.rs/tokio/latest/tokio/index.html) provides the key building blocks for actors:
[asynchronous tasks](https://docs.rs/tokio/latest/tokio/task/index.html)
and [in-memory channels](https://docs.rs/tokio/latest/tokio/sync/index.html).
Alice highlights that the issues are
*not* on the implementation of the actor processing loop,
*but* on how the actors are built, interconnected and spawn. 

But if you look at the existing Rust actor frameworks,
[actix](https://docs.rs/actix/latest/actix/) being the most notable one,
the focus is on abstracting the [actor life cycle](https://docs.rs/actix/latest/actix/trait.Actor.html#actor-lifecycle),
with specific methods to tell what to do at each stage
and [how to handle each specific message type](https://docs.rs/actix/latest/actix/trait.Handler.html).
Beyond the fact that this gives little help compared to pattern-matching on the received messages,
this adds strong constraints on how message reception and emission are interleaved,
this life cycle being controlled by the framework.
The `Handler` trait is perfect to define how to react to some input,
but how to send spontaneous messages or to arbitrarily defer a reaction?

To address the main issue - i.e. to build a mesh of connected actors,
Actix distinguishes the [`Actor`](https://docs.rs/actix/latest/actix/prelude/trait.Actor.html) from its 
[`Context`](https://docs.rs/actix/latest/actix/struct.Context.html) but tightly couples one the other.
An actix actor is unusable without an actix context and runtime.
And even assuming such a dependency, one has to define an approach on top of actix
on how to create the actor contexts, the instances and their interconnections.

Furthermore, Actix has a bias towards a request / response model of communication.
- [A response type is attached to any message](https://docs.rs/actix/latest/actix/prelude/trait.Message.html).
- [Responses are not regular messages and are sent over specific channels](https://docs.rs/actix/latest/actix/dev/trait.MessageResponse.html).
- A message handler is given a context to send arbitrary messages,
  but [this must be done before returning a response](https://docs.rs/actix/latest/actix/prelude/trait.Handler.html#tymethod.handle).

To conclude, we decided to design thin-edge actors on top of tokio without using actix.
- Tokio provides the key building blocks: asynchronous tasks and in-memory channels.
- By comparison with actor life-cycles defined and controlled by a framework, 
  we prefer the simplicity and generality of actors which behaviors are freely defined as `async fn run(&mut self)` methods.
- We need the flexibility to connect actors along various communication patterns,
  with no restrictions on the type nor the number of messages sent as a reaction to a former message,
  not even on the targets and the reaction time window.
- Our effort is to be focussed on providing a flexible but systematic approach to instantiate and connect actors.

### Approach overview

Before moving to an actor model, let's start with regular rust objects.
This will help us to stress the differences and similarities.

To implement some state-full feature interacting with peers, the typical rust recipe is to:
* wrap the state into a `struct` along peer handles,
* expose a set of methods to update this state given a `mut` reference - ensuring exclusive update access, 
* handle each object instance and peer using an `Arc::<Mutex<T>>` - allowing multiple references to an instance.

```rust
struct A { state: u64, peer: Arc::<Mutex<B>> }

impl A { 
  pub async fn do_this(&mut self, arg: ThisArg) {
    // update self.state
    self.state += 1;
    
    // interact with self.peer, acquiring the mutex first
    let mut peer = self.peer.lock().unwrap();
    peer.say("this").await;
  }
  
  pub async fn do_that(&mut self, arg: ThatArg) {
    // update self.state
    self.state += 1;

    // interact with self.peer, acquiring the mutex first
    let mut peer = self.peer.lock().unwrap();
    peer.say("that").await;
  }
}

struct B { state: u64 }

impl B {
  pub async fn say(&mut self, arg: &str) {
    self.state += 1;
    println!("{}: {}", self.state, arg);
  }
}
```

Moving to an actor model introduces two ideas:
* materialize method invocations with *messages* that can be freely cloned and serialized,
* use *channels* to *asynchronously* exchange messages between peers,
  * the actor owns the channel receiver and processes received messages,
  * the peers clone the channel sender and send messages for processing.

```rust
struct ActorA { state: u64, messages: Receiver<AMessage>, peer: Sender<BMessage> }

#[derive(Clone, Debug)]
enum AMessage {
  DoThis(ThisArg),
  DoThat(ThatArg),
}

impl A {
  pub async fn run(mut self) {
    while let Some(message) = self.messages.recv().await {
      match message {
        DoThis(arg) => {
          // update self.state
          self.state += 1;
          
          // send messages to self.peer, triggering asynchronous operations
          let _ = self.peer.send(BMessage::Say("this".to_string())).await;
        }
        DoThat(arg) => {
          // update self.state
          self.state += 1;

          // send messages to self.peer, triggering asynchronous operations
          let _ = self.peer.send(BMessage::Say("that".to_string())).await;
        }
      }
    }
  }
}

struct ActorB { state: u64, messages: Receiver<BMessage> }

#[derive(Clone, Debug)]
enum BMessage {
  Say(String),
}

impl B {
  pub async fn run(mut self) {
    while let Some(message) = self.messages.recv().await {
      match message {
        Say(arg) => {
          self.state += 1;
          println!("{}: {}", self.state, &arg);
        }
      }
    }
  }
}
```

In practice, constructing and deconstructing messages leads to boilerplate code.
To avoid that, we can wrap the message-based interface behind regular method invocations.

```rust
struct ActorHandlerA {
  sender: Sender<AMessage>
}

impl ActorHandlerA {
  pub async fn do_this(&mut self, arg: ThisArg) {
    let _ = self.sender.send(arg).await;
  }

  pub async fn do_that(&mut self, arg: ThatArg) {
    let _ = self.sender.send(arg).await;
  }
}

struct ActorA { state: AState, messages: Receiver<AMessage>, peer: ActorHandlerB }

#[derive(Clone, Debug)]
enum AMessage {
  DoThis(ThisArg),
  DoThat(ThatArg),
}

impl A {
  pub async fn run(mut self) {
    while let Some(message) = self.messages.recv().await {
      match message {
        DoThis(arg) => self.do_this(arg),
        DoThat(arg) => self.do_that(arg),
      }
    }
  }

  async fn do_this(&mut self, arg: ThisArg) {
    // update self.state
    self.state += 1;

    // interact with self.peer, without waiting for completion
    self.peer.say("this").await;
  }

  async fn do_that(&mut self, arg: ThatArg) {
    // update self.state
    self.state += 1;

    // interact with self.peer, without waiting for completion
    self.peer.say("that").await;
  }
}

struct ActorHandlerB {
  sender: Sender<BMessage>
}

impl ActorHandlerB {
  pub async fn say(&mut self, arg: &str) {
    let _ = self.sender.send(BMessage::Say(arg.to_string())).await;
  }
}

struct ActorB { state: u64, messages: Receiver<BMessage> }

#[derive(Clone, Debug)]
enum BMessage {
  Say(String),
}

impl B {
  pub async fn run(mut self) {
    while let Some(message) = self.messages.recv().await {
      match message {
        Say(arg) => self.say(arg),
      }
    }
  }

  async fn say(&mut self, arg: String) {
    self.state += 1;
    println!("{}: {}", self.state, &arg);
  }
}
```

These three variants of the same example highlight several points.

* The state is managed the same way in the three cases.
* What differs is the interaction with peers.
  * The more salient difference is related to the messages.
  * However, the main difference is the concurrency model.
    * With `Arc::<Mutex<T>>`, the caller awaits the peer to finalize the request;
      while, with messages, the requests are processed concurrently by the peer without blocking the caller.
    * In the former case, the peer can return a value to the caller,
      while, in the later case, a value cannot be easily returned
      and must be wrapped into a message sent back to the caller.
    * In the context of thin-edge, concurrent processing and asynchronous messages are preferred.
      However, we need to find a way to let the caller wait for a response when appropriate.
* The message passing approach leads to boilerplate code.
  However, the heart of the code is free of any message construction and deconstruction
  (see the methods `do_this()` and `do_that()` of the third variant).
  Furthermore, it seems feasible to generate this boilerplate code using macros
  (the `run()` method, the handler `ActorAHandler` struct and implementation, the `AMessage` enum).
* The handler type is a great place to improve both code readability and code flexibility
  (A wrapping similar to what has been done for the third variant can be done for the first to hide lock acquisition).
  We will use such handlers and adapters to encapsulate messages related features
  as sending high priority messages or sending a request for which a response is awaited.
* Similarly, the channel receiver can be encapsulated to add features
  like reading high priority messages first or awaiting a response.
* Key points are not addressed by this example.
  * Who creates the actor channels and instances?
  * How actors and peers are connected to each others?
  * Who spawns the actor `run()` method?
  * How to get the response returned by a peer?

Even if details are missing, this gives us a sketch of the pieces making an actor.

* Along its private state, an actor manipulates
  * a queue of input messages (often named the actor mailbox),
  * and handlers to actor peers to which output messages are sent to.
* The code of an actor is a loop processing input messages in turn,
  interpreting these messages as method invocations,
  updating the actor state and sending messages to peers.
* Actors are running asynchronously in background tasks and can be accessed only indirectly using handlers.
* Actor handlers and mailboxes are more than just the sending and receiving ends of message channels.
  * This is where are implemented priority and cancellation mechanisms.
  * This is a place to adapt sent messages to the type actually expected by the receiver.
  * They can also act as facades that build and send messages on regular method invocations.

## Detailed Proposal

Key design ideas:

* All the events, requests and responses that affect the behaviour of an actor (including timeouts and cancellations)
  are materialized by messages collected in a single mailbox, the actor mailbox.
  * This mailbox abstracts message gathering and prioritization over independent input channels,
    and the actor processes messages one after the other.
  * A typical mailbox encapsulates two channels:
    one for regular messages, the other one for high-priority messages as cancellations.
  * An actor implementation can provide a specific mailbox implementation,
    notably to await the response for a request sent to a peer.
  * For observability purposes, logging can be turned on/off on a mailbox
    to trace all the messages just before processing.
  * For testing purposes, the mailbox of an actor (even if specialized) can be built from a single channel
    simulating a delivery order of events, possibly interleaved with timeouts and other runtime errors.
* All the events, requests and responses sent by an actor
  are materialized by messages going through a single handler, the actor peers handler.
  * This peers handler abstracts message dispatching of a set of independent out channels.
  * An actor implementation can provide a specific peers handler implementation,
    notably to abstract away the message passing interface in favor of regular method calls.
  * For observability purposes, logging can be turned on/off on a peer handler
    to trace all the messages just sent.
  * For testing purposes, the peer handler of an actor (even if specialized) can be built onto a single channel
    gathering all the output messages in their delivery.
* The runtime itself is manipulated via messages as any actor.
  Spawning a new task or a new actor instance
  is done by sending a message to the runtime.
  When a shutdown is triggered, this request is broadcast by the runtime
  to all the running actors using a shutdown message.

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
* Abstracting channels to peers
* Abstracting channels from peers
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

We don't want to expose that the messages are sent over `mpsc` channels. 
Abstracting both the senders and receivers, we can implement specific mechanisms
to manage message priority, timeout, cancellation and more.

The receiver part is abstracted by a concrete type: a mailbox for a specific type of messages.
A key design point is that *all* the interactions with an actor must go through such a mailbox,
including runtime errors, cancellations and timeouts.
Doing so simplifies the actor event loops to `while let Some(message) = self.mailbox.next().await { ... }`
and gives the flexibility to improve the system with new message delivery mechanisms.

```rust
/// A mailbox that gather *all* the messages sent to an actor, including runtime messages.
impl<M : From<RuntimeMessage>> Mailbox<M> {
  /// Pop from the mailbox the message with the highest priority
  /// 
  /// Await till a messages is available.
  /// Return `None` when all the senders to this mailbox have been dropped and all the messages consumed.
  pub async fn next(&mut self) -> Option<M> {
    // ...
  }
  
  /// Turn on/off logging of the messages consumed from this mailbox.
  /// 
  /// The messages are logged when returned by the `next()` method.
  pub fn log_inputs(&mut self, on: bool) {
    // ...
  }
}

pub enum RuntimeMessage {
  RuntimeError(RuntimeError),
  Timeout,
  Shutdown,
  LogInputs(bool),
  LogOutputs(bool),
  // ...
}
```

The message sender of an actor is implemented as an `Address<M>`, the address of its mailbox.

```rust
/// Create a new mailbox with its address
pub fn new_mailbox<M>(buffer: usize) -> (Mailbox<M>, Address<M>) {
  // ...
}

impl<M : From<RuntimeMessage>> Address<M> {
  /// Send a message to the mailbox with that address
  /// 
  /// Await the messages is actually in the mailbox.
  /// Fail when the mailbox has been dropped.
  pub async fn send_message(&mut self, message: M) -> Result<(), ChannelError> {
    // ...
  }

  /// Clone this address which can then be used from another actor
  fn clone(&self) -> Self {
    // ...
  }
}
```

However, actors can not directly use the addresses of their peers.
If an actor sends messages of type `A` to an actor that processes messages of type `B: From<A>`,
then we do not want the source actor to be aware of the `B` message type,
and still less the unrelated message types consumed by the target as `C: Into<B>`.
Indeed, that `B` message type encompasses all the message kinds supported by the target,
and these, as the `C` type, can be completely unrelated to the source business domain that is described by the `A` type.

Instead of an `Address<B>`, the source must be provided a `Recipient<A>`,
that wraps the `Address<B>` and casts the `A` messages into `B` values under the hood.
Without such an adapter, the source actor would have to depend on the `B` type. 

![recipients](diagrams/recipients.svg)

This adaptation of addresses into recipients is done using an intermediate trait: `Sender<M>`.
The `Recipient<M>` is just a convenient way to manipulate boxes of `dyn` values.

```rust
/// A recipient for messages of type `M`
pub type Recipient<M> = Box<dyn Sender<M>>;

#[async_trait]
pub trait Sender<M>: 'static + Send + Sync {
    /// Send a message to the recipient,
    /// returning an error if the recipient is no more expecting messages
    async fn send(&mut self, message: M) -> Result<(), ChannelError>;

    /// Clone this sender in order to send messages to the same recipient from another actor
    fn recipient_clone(&self) -> Recipient<M>;
}

/// An `Address<M>` is a `Recipient<N>` provided `N` implements `Into<M>`
impl<M: Message> Address<M> {
  pub fn as_recipient<N: Message + Into<M>>(&self) -> Recipient<N> {
    self.clone().into()
  }
}

impl<M: Message, N: Message + Into<M>> Into<Recipient<N>> for Address<M> {
  fn into(self) -> Recipient<N> {
    Box::new(self)
  }
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for Address<M> {
  async fn send(&mut self, message: N) -> Result<(), ChannelError> {
    Ok(self.sender.send(message.into()).await?)
  }

  fn recipient_clone(&self) -> Recipient<N> {
    Box::new(self.clone())
  }
}
```

#### Abstracting channels to peers

In practice, an actor might have several peers, of the same type or not, known at compile time or added at runtime.
Depending on the use-case, it will be more convenient to store the peers of an actor into:
* a structure, as for the Cumulocity mapper that needs to send specific messages to specific peers,
  ```
    struct C8yMapperPeers {
      sw_manager: Recipient<SoftwareRequest>,
      op_schedule: Recipient<OperationRequest>,
    }
  ```
* a vector, as for a measurement source that broadcasts messages to any interested recipients,
  ```
    type CollectdPeers = Vec<Recipient<Measurement>>
  ```
* a map, as for a software-package manager that dispatches requests to actors each specialized on specific software type. 
  ```
    type SWManagerPeers = HashMap<String, Recipient<SoftwareRequest>>
  ```

Using a specific type appropriate to the actor is a key for code readability.
On the other hand, being able to gather in a single channel *all* the messages sent by an actor
is a key for observability testability.

The proposal is to let each actor implementation free to provide its own implementation for its peer handler,
as long as this handler can be constructed from a recipient gathering all the possible output of the actor.

```rust
pub trait Actor {
  
    /// Type of output messages this actor produces
    type Output: Message;
  
    /// Type of the peers this actor interacts with
    type Peers: From<Recipient<Self::Output>>;
    
    // ...
}
```

Note that the actor `Output` type can be built in a similar fashion as its `Input` type,
using the `fan_in_message_type!` macro.
For instance, the expression `fan_in_message_type!(C8yMapperOutput[SoftwareRequest,OperationRequest]);` expends to:

```rust
#[derive(Clone, Debug)]
enum C8yMapperOutput {
  SoftwareRequest(SoftwareRequest),
  OperationRequest(OperationRequest),
}

impl From<SoftwareRequest> for C8yMapperOutput {
    fn from(m: SoftwareRequest) -> C8yMapperOutput {
      C8yMapperOutput::SoftwareRequest(m)
    }
}

impl From<OperationRequest> for C8yMapperOutput {
    fn from(m: OperationRequest) -> C8yMapperOutput {
      C8yMapperOutput::OperationRequest(m)
    }
}
```

Similarly, the `fan_out_peer_type!` macro can be used to create the boilerplate code required
to build a struct of peers from a recipient that gather all the individual messages.

* TODO: describe the `fan_out_peer_type!` macro that creates a struct of peers each with a specific type.

Note also that each individual peer doesn't have to be represented by a `Recipient<T>` of the appropriate type.
It can be a smart handler that hides the message sending API behind regular rust methods.

```rust
struct C8yMapperPeers {
  sw_manager: SWManager,
  op_schedule: OpScheduler,
}

impl From<Recipient<C8yMapperOutput>> for C8yMapperPeers {
  fn from(recipient: Recipient<C8yMapperOutput>) -> Self {
    C8yMapperPeers {
      sw_manager: SWManager::from(recipient),
      op_schedule: OpScheduler::from(recipient),
    }
  }
}

struct SWManager {
  sender: Recipient<SoftwareRequest>
}

impl SWManager {
  pub async fn do_this(&mut self, arg: ThisArg) -> Result<(), ChannelError> {
    let req : SoftwareRequest = arg.into();
    self.sender.send(req).await
  }

  pub async fn do_that(&mut self, arg: ThatArg) -> Result<(), ChannelError> {
    let req : SoftwareRequest = arg.into();
    self.sender.send(req).await
  }
}

struct OpScheduler {
  sender: Recipient<OperationRequest>
}

impl OpScheduler {
  pub async fn say(&mut self, arg: &str) -> Result<(), ChannelError> {
    let req : OperationRequest = arg.into();
    self.sender.send(req).await
  }
}
```

To be discussed:
* Can these smart handlers be generated by a macro?

#### Abstracting channels from peers

#### Actor with no inputs or outputs

An actor can have no input messages and only acts as a source of output messages.
Similarly, an actor can have not output messages and be a sink of input messages.
Notable examples are respectively a source of measurements and a message logger.

This can be represented using an `enum` with no constructors and hence with no values.

```rust
/// An actor can have no input or no output messages
#[derive(Clone, Debug)]
pub enum NoMessage {}

impl Actor for Collectd {
  type Input = NoMessage;
  type Output = Measurement;
  
  // ...
}

impl Actor for Logger {
  type Input = LogEntry;
  type Output = NoMessage;
  
  // ...
} 
```

#### Out-of-band runtime messages

An actor should also be able to handle requests sent by the runtime, typically a shutdown request.

* For observability purposes, all these requests are materialized by messages sent to the actor.
* These requests can be handled by specific methods of the `Actor` trait, as a `shutdown()` method.
  However, it might be simpler to let the actor implementations processes these messages as any other messages.
* These runtime messages must be handled with a higher priority as regular ones.
  A shutdown request should not wait for all pending messages to be processed first.
* This priority mechanism must be encapsulated by the actor mailbox,
  to be able to improve prioritization of message delivery.
* A simple option to do that is to use two `mpsc` channels for a mailbox,
  one for high-priority messages, the other for regular messages.
* Using a [`biased tokio::select!`](https://docs.rs/tokio/latest/tokio/macro.select.html#fairness)
  ensures that high-priority messages will be delivered first to the actor.
* Sending high-priority messages could be open to regular actor peers.
  However, it must then be clear that there is a risk for regular messages to be stalled.

#### Size of the channel buffers

### Behavior

### Instantiation

#### Channel creation and ownership

### Runtime

### Discovery



