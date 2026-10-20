---
name: rust-review
description: Expert Rust code review focused on correctness, testing, and idiomatic code. Use when reviewing a diff, file, or change for bugs, test gaps, and non-idiomatic Rust.
---

Perform an expert Rust code review focusing on correctness, testing quality, and idiomatic style.
The rules are in [`CODING_STANDARDS.md`](../../../CODING_STANDARDS.md) at the repository root; this skill is the process for applying them.

**Input**: Optionally specify a file path, glob, or description (e.g., `src/mapper.rs`, `the auth changes`). If omitted, review the current working diff.

**Correctness-only mode**: the `pr-review` skill runs this skill as its Correctness axis.
In that mode it supplies the diff and lists the `CODING_STANDARDS.md` sections you own.
That list replaces the sections named under the Correctness and Testing dimensions below:
review only the listed sections (plus the extra bug hunting under Correctness), skip the Idiomatic Rust dimension, and leave the working tree untouched.

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

   Read `CODING_STANDARDS.md` and apply every rule in it to the code in scope, across three dimensions, in priority order:

   ### Correctness

   The "Correctness" and "thin-edge.io specifics" sections of `CODING_STANDARDS.md`, plus any other real bug.
   The standards list the known traps; hunt beyond them as well:

   - **Off-by-one / overflow**: integer arithmetic, iterator ranges, index calculations
   - **Lifetime issues**: returned references with incorrect lifetimes, self-referential patterns that are unsound
   - **Ordering and races**: code that only works if messages, tasks or file events arrive in a particular order
   - **Logic errors**: trace each changed branch with a concrete input and check the result is what the caller expects

   ### Testing

   The "Tests" section of `CODING_STANDARDS.md`.
   For each changed behaviour, name the test that covers it, or note what is missing.

   ### Idiomatic Rust

   The "Idiomatic Rust" section of `CODING_STANDARDS.md`, using the language features the MSRV and edition from step 2 allow.

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

   In correctness-only mode, omit the Idiomatic Rust section and the Verdict; `pr-review` gives the verdict.

   Severity tags:
   - `[critical]` — likely bug, data loss, panic, or soundness issue
   - `[warning]` — likely to cause problems, should be fixed
   - `[note]` — improvement suggestion, not blocking

**Guardrails**

- Always read MSRV and edition from `Cargo.toml` before reviewing — never assume
- Only flag real problems with clear explanations — do not invent issues
- Provide concrete suggestions and code snippets, not vague advice
- Do not re-review code that is not in scope of the diff/file specified
- If `unsafe` appears anywhere, always flag it as `[critical]` regardless of apparent correctness
- **Formatting**: outside correctness-only mode, run `just format` before reviewing so that formatting issues are fixed automatically rather than flagged. If `just format` is not available, note that the reviewer should run it before treating line-length or formatting observations as blocking.
