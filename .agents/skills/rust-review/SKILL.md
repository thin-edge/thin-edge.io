---
name: rust-review
description: Expert Rust code review focused on correctness, testing, and idiomatic code. Use when reviewing a diff, file, or change for bugs, test gaps, and non-idiomatic Rust.
---

Perform an expert Rust code review focusing on correctness, testing quality, and idiomatic style.

**Input**: Optionally specify a file path, glob, or description (e.g., `src/mapper.rs`, `the auth changes`). If omitted, review the current working diff.

**Steps**

1. **Determine what to review**

   - If a file or path is given: read that file (or files matching the glob)
   - If a description is given: use it to locate the relevant code by searching the repository, then read it
   - If nothing is given: ask the user which diff scope they want to review, defaulting to the full branch diff against `main`:
     - **Full branch vs `main`** (default) — `git diff $(git merge-base main HEAD)`. Reviews everything the branch introduces, whether committed or not. This is the right default for reviewing a PR.
     - **Working changes only** — `git diff HEAD`. Uncommitted work in the working tree.
     - **Staged only** — `git diff --staged`.

     If the merge-base cannot be resolved (e.g. the branch was not cut from `main`), fall back to `git diff HEAD`. If the chosen diff is empty, ask the user what they would like reviewed.

2. **Read the Cargo.toml files**

   Always read the **root `Cargo.toml`** of the workspace to determine:
   - `rust-version` (MSRV) — the minimum Rust version guaranteed to compile this code
   - `edition` — which Rust edition features are available

   Also read the `Cargo.toml` for any crates directly involved in the code being reviewed, to understand:
   - Which dependencies are available (e.g. `thiserror`, `tokio`, `async-trait`, `mockall`)
   - Any relevant feature flags

   Do NOT hardcode assumptions about MSRV or edition — always read them fresh.

3. **Review the code**

   Analyse the code across three dimensions, in priority order:

   ### Correctness

   Look for real bugs and logic errors. Flag anything where:

   - **Panics in non-test code**: `unwrap()` without justification, direct array/slice indexing without bounds checks. Prefer `expect("reason")` for invariants, `?` for recoverable errors.
   - **Silent error swallowing**: `let _ = result`, ignoring `Result` arms, or `.ok()` without explanation
   - **Off-by-one / overflow**: integer arithmetic, iterator ranges, index calculations
   - **Incorrect `Option`/`Result` handling**: returning `None`/`Err` where a value should always exist, or vice versa
   - **Shared state issues**: incorrect use of `Arc<Mutex<_>>`, potential deadlocks, missing synchronisation
   - **Lifetime issues**: returned references with incorrect lifetimes, self-referential patterns that are unsound
   - **Logic errors that are hard to reason about**: even if possibly correct, flag code where correctness is non-obvious and suggest simplifying or adding a comment
   - **Blocking code in async context**: synchronous operations inside `async fn` can starve the tokio executor. Flag:
     - `std::fs` calls — use `tokio::fs` instead
     - `std::net` blocking I/O — use `tokio::net`
     - `camino::Utf8Path` methods like `.exists()`, `.read_dir()`, `.metadata()` — these are thin wrappers over `std::fs` and block the thread; use `tokio::fs` equivalents (e.g. `tokio::fs::metadata()`)
     - Any other syscalls or long-running CPU work that should be moved to `tokio::task::spawn_blocking`

   **Project-specific (thin-edge.io)**:
   - **MQTT topics should go through the typed schema**, not hand-built strings. The canonical API lives in `crates/core/tedge_api/src/mqtt_topics.rs`:
     - Build publish topics with `MqttSchema::topic_for(&entity, &channel)`, using an `EntityTopicId` and a `Channel` variant.
     - Build subscription filters with `MqttSchema::topics(EntityFilter, ChannelFilter)`.
     - Parse an incoming topic back with `MqttSchema::entity_channel_of(&topic)`.

     Flag raw string-literal or `format!("te/device/...")` topics in non-test code: they bypass the schema, are easy to mistype, and don't track the configurable root prefix (`MqttSchema::root`, default `te`). When a raw topic is genuinely unavoidable, check its structure against the scheme — the entity identifier is a fixed 4-segment group (`device/<name>/<service-kind>/<service>`) followed by a channel group (e.g. `m/<type>`, `e/<type>`, `a/<type>`, `cmd/<op>/<id>`, `status/health`). The `Channel`/`ChannelFilter` enums and the `parse` method are the authoritative grammar; the doctests on `topic_for` and `topics` show worked examples.
   - Actor message ordering assumptions — verify senders and receivers are correctly wired
   - Entity store mutations — check that registration/deregistration is symmetric

   ### Testing

   - Are there tests covering the changed or added behaviour? If not, note what's missing.
   - **Test names** should describe the *behaviour* being tested, not the implementation (e.g. `returns_error_when_topic_is_empty` not `test_parse`)
   - **Focus**: each test should verify one behaviour; split tests that verify multiple things
   - **Speed and determinism**: flag sleeps (`tokio::time::sleep`, `std::thread::sleep`) in tests, flaky network usage, or non-deterministic ordering
   - **Edge cases**: empty inputs, maximum values, error paths, concurrent access — check they're covered
   - **Project conventions**:
     - Use `#[tokio::test]` for async tests
     - Inline `#[cfg(test)]` modules for unit tests in the same file
     - Separate `tests.rs` (in the same directory) for larger test suites
     - Use `pretty_assertions` for readable diffs, `test-case` for parameterised tests

   ### Idiomatic Rust

   Based on the MSRV and edition read from Cargo.toml:

   - **Prefer `?`** over explicit `match`/`if let` on `Result`/`Option` where the intent is clear
   - **Iterator adapters** over manual loops: `.map()`, `.filter()`, `.flat_map()`, `.collect()` etc. — but only where it improves readability
   - **Unnecessary clones/allocations**: `.clone()` on `Copy` types, redundant `to_string()`, avoidable `Vec` allocations
   - **`#[derive]`** everything that can be derived: `Clone`, `Debug`, `PartialEq`, `Default`, etc.
   - **Error types**: prefer `thiserror` in **library crates**, where errors are part of the public API and callers may need to match on individual variants (project convention). `anyhow` is acceptable in the **binary/application crates** that power the shipped executables — where errors are typically propagated to the top level and displayed rather than matched on — as well as in tests and examples.
   - **Naming**: follow Rust conventions — `snake_case` for functions/variables, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants
   - **`unsafe`**: flag any usage — it is forbidden in this project
   - **Edition idioms**: use `let-else` (1.65+), `if let` chains, or other features available given the actual MSRV

4. **Write the review**

   Structure the output as follows:

   ```
   ## Code Review

   **Summary**: <1-2 sentence overall assessment>

   ---

   ### Correctness

   1. [critical/warning/note] <finding> — <file>:<line if known>
      > <explanation and suggested fix, with code snippet if helpful>

   (repeat for each finding, or write "No issues found." if clean)

   ---

   ### Testing

   - <observation or suggestion>
   - ...

   (or "Test coverage looks good." if nothing to add)

   ---

   ### Idiomatic Rust

   - <observation or suggestion>
   - ...

   (or "Code is idiomatic." if nothing to add)

   ---

   **Verdict**: Approve / Request Changes / Needs Discussion
   ```

   Severity tags:
   - `[critical]` — likely bug, data loss, panic, or soundness issue
   - `[warning]` — likely to cause problems, should be fixed
   - `[note]` — improvement suggestion, not blocking

**Guardrails**

- Always read MSRV and edition from `Cargo.toml` before reviewing — never assume
- Only flag real problems with clear explanations — do not invent issues
- Provide concrete suggestions and code snippets, not vague advice
- Do not re-review code that is not in scope of the diff/file specified
- If the code under review is a test file, apply correctness and clarity checks but relax the "no `unwrap()`" rule — panicking tests are acceptable
- If `unsafe` appears anywhere, always flag it as `[critical]` regardless of apparent correctness
- **Formatting**: run `just format` before reviewing so that formatting issues are fixed automatically rather than flagged. If `just format` is not available, note that the reviewer should run it before treating line-length or formatting observations as blocking.
