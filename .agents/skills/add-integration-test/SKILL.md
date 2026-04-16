---
name: add-integration-test
description: Write a Robot Framework integration test for thin-edge.io. Use when adding end-to-end tests.
---

# Write Robot Framework Integration Test

## Steps

1. **Create `.robot` file** in `tests/RobotFramework/tests/<category>/`
1. **Add Settings section**:
   ```robot
   *** Settings ***
   Resource    ../../resources/common.resource
   Library     ThinEdgeIO
   Library     Cumulocity  # For interacting with the device management from Cumulocity

   Test Setup       Setup
   Test Teardown    Get Logs
   ```
1. Import `Library    Cumulocity` while testing device integration with Cumulocity,
   like asserting telemetry or inventory data sent by the device to the cloud or
   to trigger device management operations from the device to the cloud
1. **Add test tags**:
   ```robot
   Test Tags    theme:<category>
   ```
1. **Write test cases**:
   ```robot
   *** Test Cases ***
   My Test Case Description
       Execute Command    tedge mqtt pub te/device/main///m/test '{"value": 42}'
       # Use assertion keywords to verify behavior
   ```
1. **Run with**: `just integration-test --test PATTERN`

## Debug Test

1. Identify the test container (simulating the tedge device) started by the test: `docker ps -q --filter ancestor=debian-systemd | tail -n1`
1. There could be other test containers using the `debian-systemd` image, simulating child devices as well
1. Execute shell commands on the test containers spawned by the test.
1. Fetch service logs using `journalctl`:
   - C8Y mapper: `journalctl -f -u tedge-mapper-c8y`
   - Tedge agent: `journalctl -f -u tedge-agent`
   - Complete MQTT message trace: `journalctl -f -u mqtt-logger`

## Notes

- **Device adapters**: `docker` (CI default), `ssh` (remote devices), `local` (localhost)
- **Environment**: `.env` file with cloud credentials (template at `tests/RobotFramework/devdata/env.template`)
- Thin Edge specific test keywords are defined in `tests/RobotFramework/libraries/ThinEdgeIO/ThinEdgeIO.py`
- Shared variables/resources in `tests/RobotFramework/resources/common.resource`

## Reference Files

Read these for patterns:
- `tests/RobotFramework/tests/tedge_write.robot` — simple test structure
- `tests/RobotFramework/tests/cumulocity/log/log_operation.robot` — for interactions with Cumulocity
- `tests/RobotFramework/resources/common.resource` — shared variables and resource setup
- `tests/RobotFramework/libraries/ThinEdgeIO/ThinEdgeIO.py` — available keywords
