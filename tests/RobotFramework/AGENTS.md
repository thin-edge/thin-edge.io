# Integration Test Reference

## Directory Organization

Tests are organized by feature area under `tests`:
- Each `.robot` file covers a specific feature or scenario
- Tag tests with `theme:<category>` for filtering

## Device Adapters

- **Docker** (`adapter=docker`) — default for CI, runs tests in containers
- **SSH** (`adapter=ssh`) — tests on remote physical/virtual devices
- **Local** (`adapter=local`) — tests against localhost

## ThinEdgeIO Library

The `ThinEdgeIO` Python library (`libraries/ThinEdgeIO/ThinEdgeIO.py`) provides keywords for:
- Executing commands on the device
- Managing services (start, stop, restart)
- MQTT operations (publish, subscribe)
- File operations and assertions

## Resources

- `resources/common.resource` — shared variables (`${DEVICE_ADAPTER}`, cloud configs) with environment variable defaults
- Variables use `%{ENV_VAR}` syntax for environment variable substitution

## Environment Setup

- Copy `devdata/env.template` to `.env` and fill in cloud credentials
- Run `just setup-integration-test` for first-time setup
- Use `tasks.py` invoke commands for test orchestration

## Running Tests

```bash
just integration-test                  # all tests
just integration-test --test PATTERN   # specific tests
```
