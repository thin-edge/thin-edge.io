# AGENTS.md

This file provides guidance for AI coding agents working with code in this repository.

## Project Overview

thin-edge.io is an open-source, cloud-agnostic edge framework for resource-constrained IoT devices. It's written in Rust and provides device management, telemetry, and multi-cloud connectivity (AWS, Azure, Cumulocity) for industrial IoT applications.

**Key characteristics:**
- Actor-based architecture using async Rust with tokio
- MQTT as the primary inter-process communication mechanism
- Plugin system for extensibility
- Designed for resource-constrained devices (low RAM, embedded Linux)
- Minimum Rust version: 1.85

## Common Commands

### Building

```bash
# Build all components (creates binaries and packages)
just release

# Build for specific target (cross-compilation supported automatically)
just release TARGET

# Example targets:
# - x86_64-unknown-linux-musl
# - aarch64-unknown-linux-musl
# - armv7-unknown-linux-musleabihf
```

### Testing

```bash
# Run all unit and doc tests
just test

# Run only unit tests
just test-unit

# Run specific test
cargo nextest run --status-level fail --all-features test_name

# Run doc tests only
just test-docs

# Setup integration test environment (one-time setup)
just setup-integration-test

# Run integration tests (builds first, then runs Robot Framework tests)
just integration-test

# Run specific integration test suite
just integration-test --test PATTERN
```

### Code Quality

```bash
# Format code (includes Rust, TOML, and Robot Framework tests)
just format

# Check formatting without modifying files
just format-check

# Run linting (clippy + dependency checks)
just check

# Check for specific target
just check TARGET

# Run clippy manually
cargo clippy --all-targets --all-features

# Check dependencies (licenses, security advisories)
just check-dependencies
```

### Development Setup

```bash
# One-time setup: install required tools and configure git hooks
just install-tools
just prepare-dev

# The prepare-dev command adds a git pre-commit hook that automatically
# adds the required "Signed-off-by" trailer to commits (required by CLA)
```

## Architecture

### High-Level Design

thin-edge.io uses a **distributed actor-based architecture** where components communicate via:
1. **Local MQTT broker** (rumqttd) - primary IPC mechanism on localhost:1883
2. **Actor message passing** - type-safe async channels within processes

**Core concepts:**
- **Actors**: Independent, concurrent units that process messages
- **Runtime**: Manages actor lifecycle, graceful shutdown, and crash handling
- **Message boxes**: Type-safe channels connecting actors
- **Builders**: Declarative actor composition with compile-time guarantees

### Directory Structure

```
crates/
├── core/               # Essential components (always included)
│   ├── tedge/          # Main CLI binary (multicall pattern)
│   ├── tedge_actors/   # Actor framework (minimal dependencies)
│   ├── tedge_mapper/   # Cloud data translation engine
│   ├── tedge_agent/    # Cloud-agnostic operations handler
│   ├── tedge_api/      # Domain models and APIs
│   ├── plugin_sm/      # Plugin manager for software operations
│   ├── tedge_write/    # File writing service
│   └── tedge_watchdog/ # Process monitoring
├── common/             # Utility libraries
│   ├── mqtt_channel/   # MQTT client wrapper
│   ├── tedge_config/   # Configuration management
│   ├── download/       # File download utilities
│   ├── upload/         # File upload utilities
│   └── ...
├── extensions/         # Optional feature modules (26+ crates)
│   ├── *_mapper_ext/   # Cloud provider mappers (c8y, aws, az)
│   ├── tedge_mqtt_ext/ # MQTT actor
│   ├── tedge_flows/    # JavaScript flow engine (QuickJS)
│   ├── tedge_*_ext/    # Various actor extensions
│   └── ...
└── tests/              # Shared testing utilities

plugins/                # Plugin binaries (separate processes)
├── tedge_apt_plugin/   # Package management (apt)
├── c8y_*_plugin/       # Cumulocity-specific plugins
└── tedge_file_*_plugin/# File operations plugins

tests/RobotFramework/   # End-to-end system tests
```

### Main Binaries

The project builds a **multicall binary** that acts as different executables based on its name or command:

1. **`tedge`** - Main CLI tool
   - Entry point: `crates/core/tedge/src/main.rs`
   - Commands: `tedge run`, `tedge config`, `tedge cert`, etc.
   - Can spawn mapper, agent, and other components

2. **`tedge-mapper`** - Cloud protocol translator
   - Runs as: `tedge run mapper [c8y|aws|az|collectd]`
   - Translates between thin-edge.io data model and cloud-specific formats
   - Each cloud provider has its own mapper implementation

3. **`tedge-agent`** - Device operations handler
   - Runs as: `tedge run agent`
   - Handles: software management, restart, config updates, log uploads
   - Orchestrates plugins for specific operations

### MQTT Topic Hierarchy

```
te/device/main/                       # Main device
te/device/main/service/tedge-agent/   # Service registration
te/device/main/cmd/restart/+          # Commands (inbound)
te/device/main/status/restart/+       # Status (outbound)
tedge/commands/req/operation/+        # Cloud-agnostic commands
tedge/commands/res/operation/+        # Cloud-agnostic responses

# Cloud-specific topics are handled by mappers
c8y/...                               # Cumulocity topics
aws/...                               # AWS IoT topics
az/...                                # Azure IoT topics
```

### Mappers: Cloud Integration Layer

Mappers translate between thin-edge.io's cloud-agnostic data model and cloud provider-specific formats.

**Data flow:**
```
Device → te/* topics → Mapper → Cloud-specific topics → MQTT Bridge → Cloud
Cloud → Mapper → te/* topics → Agent/Services
```

**Mapper implementations:**
- **c8y_mapper_ext**: Cumulocity IoT (most feature-complete)
- **aws_mapper_ext**: AWS IoT Core (flows-based)
- **az_mapper_ext**: Azure IoT Hub (flows-based)
- **collectd_ext**: Collectd metrics integration

**Key mapper features:**
- Bidirectional translation (cloud ↔ device)
- Flow-based transformations (JavaScript rules via QuickJS)
- File upload/download coordination
- Health monitoring and auto-reconnection
- Operation state management

### Plugins: Process Isolation for Operations

Plugins are **separate executables** (not dynamically loaded libraries) that handle specific operations.

**Plugin execution model:**
1. Agent receives operation via MQTT
2. Agent selects plugin based on operation type
3. Agent spawns plugin as child process with CLI args
4. Plugin executes (uses system commands, modifies files, etc.)
5. Plugin writes logs to files
6. Agent reads logs and publishes status to MQTT

**Built-in plugins:**
- `tedge-apt-plugin`: System package management
- `tedge-file-config-plugin`: Configuration file management
- `tedge-file-log-plugin`: Log file uploads
- `c8y-firmware-plugin`: Firmware updates
- `c8y-remote-access-plugin`: Remote SSH/VNC access

**Plugin benefits:**
- Process isolation (crash safety)
- Language flexibility (can be shell scripts)
- Independent versioning

## Code Style and Conventions

### General Guidelines

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- **Avoid unsafe code** unless absolutely necessary
- Prefer standard library and established crates over custom implementations
- Use declarative builder patterns for actor composition

### Error Handling

From CODING_GUIDELINES.md:
- Use `Result<T, E>` for recoverable errors
- **DO NOT use `panic!()`** - code should not panic
- **Avoid `unwrap()`** except for:
  - Mutexes: `lock().unwrap()` is acceptable
  - Test code
  - Custom error messages: `.unwrap_or_else(|| panic!("error: {}", foo))`
- **Prefer `expect()`** over `unwrap()` with detailed error messages
- Use `assert!()` to protect system invariants (kept in release builds)

### Actor Development Patterns

When creating new actors:

1. **Define clear message types:**
   ```rust
   pub struct MyRequest { /* fields */ }
   pub struct MyResponse { /* fields */ }
   ```

2. **Implement appropriate actor trait:**
   - `Server` - Request-response pattern
   - `Actor` - Custom message loop
   - `MessageSource`/`MessageSink` - Stream processing

3. **Use builders for composition:**
   ```rust
   let mut builder = MyActorBuilder::new(config);
   builder.connect_sink(other_actor);
   let actor = builder.build();
   runtime.spawn(actor).await?;
   ```

4. **Handle graceful shutdown:**
   - Listen for `RuntimeRequest::Shutdown`
   - Clean up resources
   - Return `Ok(())` or appropriate error

## Testing

### Test Organization

- **Unit tests**: In the same file as code or in `tests.rs` module
- **Integration tests**: `crates/tests/` directory
- **End-to-end tests**: `tests/RobotFramework/` (Python-based)

### Testing Actor-Based Code

When testing actors:
- Use message boxes directly to inject test messages
- Use `tokio::time::timeout()` to prevent hanging tests
- Mock external dependencies (MQTT, HTTP, file system)
- Test crash recovery and error paths

### Robot Framework Tests

Integration tests use Robot Framework with custom device adapters:
- **Local adapter**: Tests against localhost
- **Docker adapter**: Tests in containers
- **SSH adapter**: Tests on remote devices

Setup requires `.env` file with cloud credentials (see `tests/RobotFramework/devdata/env.template`).

## Git Workflow and Commits

### Commit Requirements

**MANDATORY for all commits:**
- Include `Signed-off-by` trailer (use `git commit -s`)
- Subject line: ≤50 characters, capitalized, imperative mood, no period
- Empty line after subject
- Body lines: ≤72 characters
- Explain **why** the change was made (not just what)

**Automated checks enforce:**
- `Signed-off-by` trailer present
- Commit message formatting
- No `fixup!` or `squash!` commits when merging

**Run `just prepare-dev` to add a git hook that automatically signs commits.**

### Pull Request Guidelines

- **One issue per PR** - keep scope focused
- **Do NOT use GitHub auto-close keywords** (Fixes #123) - testers close issues manually
- **Use `git commit --fixup=<sha>`** for review feedback
- **Squash fixup commits before merging** (`git rebase -i --autosquash main`)
- **At least one maintainer approval** required
- **Merging via bors-ng bot** - prevents merge skew

### License and Dependencies

- All code is Apache 2.0 licensed
- License linting via `cargo-deny` (run with `just check-dependencies`)
- Dependencies must be compatible with Apache 2.0
- Keep dependencies synchronized across workspace crates

## Common Development Patterns

### Adding a New Cloud Mapper

1. Create extension crate: `crates/extensions/xyz_mapper_ext/`
2. Implement `TEdgeComponent` trait with `start()` method
3. Create `XyzMapper` struct implementing actor composition
4. Define MQTT topic mappings and converters
5. Add flow definitions if using flow-based approach
6. Register mapper in `tedge_mapper::bin/tedge_mapper.rs`
7. Add integration tests in `tests/RobotFramework/`

### Adding a New Plugin

1. Create binary crate: `plugins/my_plugin/`
2. Implement `Plugin` trait from `plugin_sm` crate
3. Define CLI interface (install, remove, list, version)
4. Add plugin registration in agent configuration
5. Write unit tests for plugin logic
6. Add Robot Framework tests for end-to-end validation

### Adding Configuration Options

1. Define option in `tedge_config` using `define_tedge_config!` macro
2. Add validation and default values
3. Update `tedge config` CLI to expose new option
4. Add tests for configuration parsing and validation
5. Update documentation (in `docs/` directory)

## Troubleshooting

### Build Issues

- **Cross-compilation fails**: Install `cargo-zigbuild` for better cross-compilation support
- **Linker errors on musl**: Check that you're using the correct musl toolchain
- **Dependency conflicts**: Run `cargo update` and `cargo tree` to debug

### Test Issues

- **Integration tests fail**: Ensure `.env` file is configured with valid cloud credentials
- **Robot Framework not found**: Run `just setup-integration-test`
- **Tests hang**: Check that local MQTT broker (rumqttd) is running and port 1883 is available

### Runtime Issues

- **Actors crash**: Check logs in `~/.tedge/logs/`
- **MQTT connection fails**: Verify broker is running on localhost:1883
- **Plugin execution fails**: Check plugin logs in `~/.tedge/operations/`

## Additional Resources

- **Documentation**: https://thin-edge.github.io/thin-edge.io/
- **Design documents**: `design/` directory
  - `thin-edge-actors-design.md` - Actor framework architecture
  - `thin-edge-core.md` - Core component design
- **Vision and goals**: `vision.md`
- **Contributing guide**: `CONTRIBUTING.md`
- **Coding guidelines**: `CODING_GUIDELINES.md`
