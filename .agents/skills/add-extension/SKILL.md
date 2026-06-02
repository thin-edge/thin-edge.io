---
name: add-extension
description: Add a new extension of `tedge-mapper` or `tedge-agent` as an actor.
---

# Add Extension

## Steps

1. **Create extension actor crate** `crates/extensions/<name>/` with `src/lib.rs`, `src/actor.rs`, `src/builder.rs`, `src/messages.rs`
1. **Generate `Cargo.toml`** with workspace conventions:
   ```toml
   [package]
   name = "<name>"
   version.workspace = true
   edition.workspace = true
   license.workspace = true

   [lints]
   workspace = true

   [dependencies]
   tedge_actors = { workspace = true }
   # add other workspace deps as needed
   ```
1. **Define message types** in `messages.rs` (implement `Message: Debug + Send + Sync + 'static`)
1. **Define actor** in `actor.rs`:
   - Struct with a message box field (e.g., `ServerMessageBox<Request, Response>` or `SimpleMessageBox<Input, Output>`)
   - Implement `Actor` trait: `fn name(&self) -> &str`, `async fn run(self) -> Result<(), RuntimeError>`
1. **Define builder** in `builder.rs`:
   - Implement `Builder<T>`, `RuntimeRequestSink`
   - Implement `MessageSink<M>` and `MessageSource<M, Config>` for wiring
   - Use `SimpleMessageBoxBuilder` (event-driven) or `ServerMessageBoxBuilder` (request-response)
1. **Add crate to workspace** `Cargo.toml` — extensions use glob (`crates/extensions/*`) so no manual addition needed
1. **Register with the agent** in `crates/core/tedge_agent/src/agent.rs`:
1. **Verify**: Run `just check` and `just test`

## Reference Files

Read these for patterns:
- `crates/extensions/tedge_timer_ext/` — server-based actor with `ServerMessageBox`, builder, and tests
- `crates/extensions/tedge_signal_ext/src/lib.rs` — minimal actor using `SimpleMessageBox`
- `crates/core/tedge_mapper/src/lib.rs` — mapper registration and `lookup_component()`
