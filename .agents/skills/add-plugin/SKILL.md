---
name: add-plugin
description: Add a new plugin binary to thin-edge.io. Use when creating a new extension of built-in operations supported by tedge-agent.
---

# Add Plugin Binary

## Steps

1. **Create crate directory** `plugins/<name>/` with a `src/lib.rs` and `src/bin.rs` (and optionally `src/main.rs`).
   The `main.rs` file is used only for integration testing.
   The real plugin invocation  happens via the `tedge` multi-call binary.
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
   clap = { workspace = true }
   # add other workspace deps as needed
   ```
1. **Add to workspace members** in root `Cargo.toml` — plugins are listed explicitly (not globbed):
   ```toml
   members = [
       # ...existing...
       "plugins/<name>",
   ]
   ```
1. **Define CLI** with `#[derive(clap::Parser)]` and a `PluginOp` subcommand enum:
   ```rust
   #[derive(clap::Subcommand)]
   pub enum PluginOp {
       Subcommand1,
       Subcommand2
       # ...remaining supported subcommands
   }
   ```
1. **Implement `run()`** in the `bin.rs` that handles each subcommand variant and returns an `anyhow::Result<()>`.
1. Add this library as a dependency of the `crates/core/tedge` multi-call binary crate.
1. Call this `run()` from the multi-call binary definition at `crates/core/tedge/src/main.rs`.
1. **Verify**: Run `just check` and `just test`

## Reference Files

Read these for patterns:
- `plugins/tedge_apt_plugin/src/lib.rs` — library-only pattern with clap CLI and `run_and_exit()`
- `plugins/tedge_file_log_plugin/src/main.rs` — bin.rs + lib.rs pattern
