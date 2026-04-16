# Actor Framework Reference

## Actor Traits

Choose the right trait for your use case:
- **`Actor`** — Custom message loop. Use when you need full control over message processing (e.g., timers, signal handling, complex state machines).
- **`Server`** — Request-response pattern. Use when each incoming message produces exactly one response.

## Message Box Types

- **`SimpleMessageBox<Input, Output>`** — Event-driven, fire-and-forget. Builder: `SimpleMessageBoxBuilder`.
- **`ServerMessageBox<Request, Response>`** — Request-response with backpressure. Builder: `ServerMessageBoxBuilder`.

## Message Type Constraints

All message types must implement: `Debug + Send + Sync + 'static`

## Fan-In Macro

Combine multiple input message types into one enum:
```rust
fan_in_message_type!(CombinedMsg[MsgA, MsgB] : Clone, Debug);
```

## Builder Traits

Every actor builder must implement:
- `Builder<MyActor>` — `fn try_build(self) -> Result<MyActor, ...>`
- `RuntimeRequestSink` — `fn get_signal_sender(&self) -> DynSender<RuntimeRequest>`

For wiring:
- `MessageSink<M>` — actor can receive messages of type M
- `MessageSource<M, Config>` — actor can send messages of type M

## Test Helpers

- `SimpleMessageBoxBuilder` — create test harnesses for actors
- `MessageReceiverExt` — convenience methods for receiving messages in tests
- `FakeServerBox` / `FakeServerBoxBuilder` — mock server actors
- Always use `tokio::time::timeout()` in tests to prevent hanging

## Further Reading

- `design/thin-edge-actors-design.md` — architecture and design rationale
- `crates/core/tedge_actors/src/lib.rs` — full API documentation
