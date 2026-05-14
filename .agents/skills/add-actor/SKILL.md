---
name: add-actor
description: Create a new actor with builder in the thin-edge.io actor framework. Use when implementing a new concurrent component that processes messages.
---

# Create Actor with Builder

## Steps

1. Create a new Rust module under `crates/extensions`
1. **Define message types** in `messages.rs`:
   - All messages must implement `Message: Debug + Send + Sync + 'static`
   - Request/response pairs for `Server` pattern, or input/output for `Actor` pattern
1. Use fan-in macro for multiple input message types:
   ```rust
   fan_in_message_type!(CombinedMsg[MsgA, MsgB] : Clone, Debug);
   ```
1. **Create actor struct** with a message box field:
   ```rust
   pub struct MyActor {
       message_box: SimpleMessageBox<InputMsg, OutputMsg>,
       // or: message_box: ServerMessageBox<Request, Response>,
   }
   ```
1. **Implement `Actor` trait**:
   - `fn name(&self) -> &str` — unique identifier
   - `async fn run(self) -> Result<(), RuntimeError>` — main message loop
1. **Create builder** using appropriate message box builder:
   - `SimpleMessageBoxBuilder` — for event-driven actors (fire-and-forget)
   - `ServerMessageBoxBuilder` — for request-response actors
1. **Implement builder traits**:
   - `Builder<MyActor>` — `fn try_build(self) -> Result<MyActor, ...>`
   - `RuntimeRequestSink` — `fn get_signal_sender(&self) -> DynSender<RuntimeRequest>`
   - `MessageSink<M>` — for each input message type
   - `MessageSource<M, Config>` — for each output message type
1. **Wire into parent** using builder methods:
   ```rust
   builder.connect_sink(other_builder.get_sender());
   builder.connect_source(other_builder);
   ```
1. **Write tests** using `SimpleMessageBoxBuilder` test harnesses

## Reference Files

Read these for patterns:
- `crates/core/tedge_actors/src/lib.rs` — framework docs and all public API
- `crates/extensions/tedge_timer_ext/` — complete actor with server message box, builder, and tests
- `crates/extensions/tedge_signal_ext/src/lib.rs` — minimal actor with simple message box
- `design/thin-edge-actors-design.md` — architecture and design rationale
