# Coding Standards

These are the standards that thin-edge.io code is reviewed against.
They build on [CODING_GUIDELINES.md](CODING_GUIDELINES.md)
and are the rules that the `pr-review` and `rust-review` skills (`.agents/skills/`) check.
Many of these points are things that AI-generated code tends to get wrong,
so check them carefully whether you or an agent wrote the code.

Rules tagged **(polish)** are worth fixing in your own code before opening a pull request,
but are not worth asking someone else to change in a review.
Untagged rules apply to every change.

## Toolchain and formatting

- The MSRV and edition are set in the root `Cargo.toml` (`rust-version`, `edition`).
  Read them rather than assuming; use any language feature the MSRV allows (e.g. `let`-`else`).
- Format with `cargo +nightly fmt --all` (or `just format`).
  `rustfmt.toml` sets `imports_granularity = "Item"`, which is nightly-only:
  stable rustfmt prints a warning and leaves imports unformatted, so CI will disagree with you.
- Imports are one item per `use` line, sorted.
- Run `just check` (clippy plus dependency checks) before pushing.
  `clippy.toml` forbids some types and methods; read the `reason` instead of suppressing the lint.
  For example, `reqwest::Client::new` and `reqwest::Client::builder` are banned
  because they ignore the configured root certificates; use `certificate::CloudRootCerts`.
- Workspace crates take `version`, `edition`, `license` and dependency versions from the workspace
  (`{ workspace = true }`) and set `[lints] workspace = true`.
- Use inline format arguments: `format!("{key}")`, not `format!("{}", key)`.
  Clippy allows both, but new code uses the inline form. **(polish)**

## Correctness

### Panics

- No `unwrap()` in non-test code, except on mutex locks (`lock().unwrap()`).
- Use `expect("why this cannot fail")` for real invariants.
  The message should state the invariant, not just repeat the call.
- Use `?` for recoverable errors.
- No unchecked slice or array indexing where the index comes from input.
  Use `.get()` and handle `None`.
- No `todo!()` or `unimplemented!()` in shipped code paths.
- `unsafe` code is denied by the workspace lints.
  Where it cannot be avoided, the item carries `#[allow(unsafe_code)]`
  and the block has a `// SAFETY:` comment stating why it is sound.

### Errors

- Do not silently drop errors.
  `let _ = result`, `.ok()` and empty `Err(_) => {}` arms need a comment explaining why ignoring the error is correct.
  Most of the time the error should be logged or propagated instead.
- Library crates define errors with `thiserror`,
  so callers can match on variants.
  Compose them with `#[from]` and `#[error(transparent)]`.
- `anyhow` is fine in binary/application crates, tests and examples,
  where errors are shown to the user rather than matched on.
- A plain `String` error is fine where the message goes to the user as it is and no caller matches on it,
  such as `FromStr` for a config value type.
- Error messages say what failed and on what (include the path, topic or operation id).
- Error messages start with a capital letter. **(polish)**
- Watch `Option`/`Result` semantics:
  don't return `None` where a value must exist, and don't invent a default where absence is a real case.

### Async code

Blocking calls inside `async fn` starve the tokio executor.
In async code:

- Use `tokio::fs`, not `std::fs`.
- Use `tokio::net`, not blocking `std::net` I/O.
- Do not call `camino::Utf8Path` methods that touch the filesystem
  (`.exists()`, `.is_file()`, `.read_dir()`, `.metadata()`).
  They are thin wrappers over `std::fs` and block the thread.
  Use the `tokio::fs` equivalents, e.g. `tokio::fs::metadata()`.
- Move other long-running syscalls or CPU-heavy work to `tokio::task::spawn_blocking`.
- Use `tedge_utils::fs::atomically_write_file_async` (or `_sync` in sync code)
  to write files that other processes may read, so they never see a half-written file.

### Shared state

- Check `Arc<Mutex<_>>` usage for deadlocks
  and for locks held across `.await` points.
- Prefer passing messages between actors to sharing state between them.

### Clarity

If the correctness of a piece of code is not obvious, it is a problem even if the code is correct.
Simplify it, or add a comment explaining why it is correct.

## thin-edge.io specifics

### MQTT topics

Build MQTT topics through the typed schema in `crates/core/tedge_api/src/mqtt_topics.rs`,
not with string literals or `format!("te/device/...")`:

- Publish topics: `MqttSchema::topic_for(&entity, &channel)`,
  with an `EntityTopicId` and a `Channel` variant.
- Subscriptions: `MqttSchema::topics(EntityFilter, ChannelFilter)`.
- Parsing incoming topics: `MqttSchema::entity_channel_of(&topic)`.

Hand-built topics are easy to mistype
and ignore the configurable root prefix (`MqttSchema::root`, default `te`).
If a raw topic really is unavoidable,
it must follow the scheme:
a fixed 4-segment entity identifier (`device/<name>/<service-kind>/<service>`)
followed by a channel group (`m/<type>`, `e/<type>`, `a/<type>`, `cmd/<op>/<id>`, `status/health`).
The `Channel`/`ChannelFilter` enums are the authoritative grammar.
Test code may use literal topics.

### Actors and the entity store

- Check that actor senders and receivers are wired to each other,
  and that the code does not assume a message ordering the runtime does not guarantee.
  See `.agents/skills/add-actor` when creating actors.
- Entity store registration and deregistration must be symmetric:
  anything added on registration is removed on deregistration.

### Paths and logging

- Use `camino::Utf8Path`/`Utf8PathBuf` for paths, not `std::path`.
- Use `tracing` for logging, not `log` or `println!`/`eprintln!` (outside CLI output).

## Tests

### What to test

- Every change in behaviour has a test.
  Bug fixes include a test that fails without the fix.
- Cover edge cases: empty input, maximum values, error paths, concurrent access.
- Each test checks one behaviour.
  Split tests that check several unrelated things.
- When working test-first, run the new tests before implementing the behaviour,
  and check that they fail on their assertions.
  A test that fails only because the code does not compile yet has not been checked. **(polish)**

### Naming and layout

- Name tests after the observable behaviour, not the implementation:
  `does_not_forward_until_subscribed`, not `gated_publishes_are_withheld_until_gate_opens` or `test_parse`.
  Avoid internal vocabulary ("gate", "buffer", "watch channel") in test names. **(polish)**
- No doc comments (`///`) on test functions.
  If a test needs one, the name is not descriptive enough.
  Inline `//` comments explaining a non-obvious step are fine.
  CI rejects doc comments on new and changed tests; `just check-test-docs` runs the same check. **(polish)**
- Unit tests go in an inline `#[cfg(test)] mod tests`,
  or in a `tests.rs` next to the module when they get long. **(polish)**
- Helper functions go at the bottom of the test module, after all the tests. **(polish)**

### Speed and determinism

- No `sleep` in tests (`tokio::time::sleep`, `std::thread::sleep`).
  Wait on the event you care about instead.
  For timer-driven logic, use `#[tokio::test(start_paused = true)]` and let tokio advance the clock.
- Every await on something that may never arrive has a timeout,
  so a broken test fails instead of hanging.
  Use `tokio::time::timeout` or the `with_timeout` helpers in `tedge_actors::test_helpers`.
- No real network access and no dependence on non-deterministic ordering (e.g. `HashMap` iteration).

### Tools

- `#[tokio::test]` for async tests.
- `rstest` for parameterised tests (`test-case` is also used, but `rstest` is preferred for new tests).
- `pretty_assertions` for readable diffs, `assert_matches!` for matching on enum shapes. **(polish)**
- `tedge_test_utils::fs::TempTedgeDir` for temporary config and data directories.
- Actor tests inject messages through message boxes
  (`SimpleMessageBoxBuilder` and `MessageReceiverExt`) rather than running real MQTT or HTTP.
- `unwrap()` is fine in tests.

## Idiomatic Rust

The rules in this section are **(polish)**, except where following one has a clear payoff:

- it is a small change that makes the code clearly simpler;
- it removes a substantial amount of code;
- or it makes the logic easier to follow, such as factoring a block into a well-named function.

A suggestion with that payoff is not polish, and it says what the payoff is.

- Prefer `?` to `match`/`if let` that only re-raises the error.
- Prefer `let`-`else` to nested `if let` for early returns.
- Use iterator adapters (`map`, `filter`, `flat_map`, `collect`) where they read better than a loop.
  Don't force them where a loop is clearer.
- Avoid needless allocation: `.clone()` on `Copy` types, redundant `.to_string()`,
  `Vec`s that are built only to be iterated once.
- Derive what can be derived (`Debug`, `Clone`, `PartialEq`, `Default`, ...).
- Keep visibility as narrow as possible; use `pub(crate)` for crate-internal items.

## Comments and documentation

- Read each new or changed comment cold, as someone who never saw the review discussion:
  it must be true, and every step from cause to effect must be present.
- Comments explain *why*, not *what*.
  When shortening a comment, keep the reasoning. **(polish)**
- Each sentence of a comment prevents a misreading:
  without it, a reader would misunderstand the code or "fix" it wrongly.
  Background, edge-case analysis and answers to review questions belong in the commit message or the review reply. **(polish)**
- A comment that justifies a workaround (an ignored error, a hand-rolled replacement for a standard API, a "not X because…") is a lead, not an answer.
  Follow its reasoning back to the premise and ask whether the code needs that premise at all.
  If the workaround only exists to keep doing something pointless, remove the pointless code and the workaround with it.
  Explain behaviour from the point of view of the system,
  e.g. "the cloud connection uses a clean session, so rumqttc discards pending messages; the bridge still has to deliver them".
- Write plainly.
  Many contributors read English as a second language,
  so avoid idioms, allusions and invented names for things. **(polish)**
- In docs, a few short sentences read better than one long sentence with many clauses.
  Use a bulleted list for items that really are separate (e.g. which signal triggers which action),
  not to split up a single sentence. **(polish)**
- Doc comments use the third person ("Iterates over the entities", not "Iterate over the entities").
  Short single-phrase comments have no trailing full stop;
  multi-sentence docs use normal punctuation. **(polish)**
- Document only what is not obvious.
  State a contract once, on the most visible item, and point to it from elsewhere. **(polish)**
- Don't link to private items from public docs; rustdoc warns about it.
  Use a plain code span instead. **(polish)**
- Markdown uses [semantic line breaks](https://sembr.org/). **(polish)**
- Design documents, specs and guides describe the current design.
  They don't refer to their own earlier versions ("earlier versions of this guide said…"),
  because readers have not seen them.
  The reasons for rejecting an alternative design are fine to include.
- Design documents and specs state decisions.
  Settle open questions before writing them down, rather than recording the uncertainty.

## Commits and pull requests

- Commits are signed off (`git commit -s`) and follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
- One issue per pull request.
- Don't use GitHub auto-close keywords (`Fixes #123`); testers close issues by hand.
- Address review feedback with `git commit --fixup=<sha>`
  and squash the fixups before merging.
