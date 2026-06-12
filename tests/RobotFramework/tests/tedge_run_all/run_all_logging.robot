*** Settings ***
Documentation       Live log-level reload for the single-process supervisor (`tedge run all`).
...                 On SIGHUP the supervisor re-reads the per-component `[log]` levels from
...                 `system.toml` and applies them without restarting any component, unless
...                 an explicit override (`--log-level`/`--debug`/`RUST_LOG`) is in effect.
...                 The change is proven from the supervisor's journal: the MQTT actor logs
...                 received messages at DEBUG, so a published measurement leaves a
...                 `MQTT recv` line only once the component has been raised to `debug`.

Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Register Device
Test Teardown       Stop Supervisor

Test Tags           theme:supervisor    theme:c8y


*** Test Cases ***
SIGHUP applies updated log levels live
    [Documentation]    Raising a component's level in system.toml and sending SIGHUP makes
    ...    the new level take effect immediately, with no component restart.
    Start Supervisor
    ${before}=    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    # Baseline: at the default INFO level the mapper does not log the messages it
    # receives, so a published measurement leaves no `MQTT recv` line behind.
    ${start}=    Get Unix Timestamp
    Trigger Mapper Mqtt Activity
    Sleep    2s    reason=Give the mapper time to (not) log the received message
    Supervisor Log Should Not Contain    MQTT recv    date_from=${start}

    # Operator raises the mapper to debug and signals a reload.
    Set Component Log Level    tedge-mapper-c8y    debug
    Execute Command    cmd=systemctl kill --signal=SIGHUP tedge-run-all.service
    Wait Until Keyword Succeeds
    ...    30s    2s    Supervisor Log Should Contain    log levels reloaded

    # After the reload the same publish is now recorded at DEBUG, proving the new
    # level applies to subsequent records without a restart.
    ${after}=    Get Unix Timestamp
    Trigger Mapper Mqtt Activity
    Wait Until Keyword Succeeds
    ...    30s    2s    Supervisor Log Should Contain    MQTT recv    date_from=${after}

    # No component was restarted: the mapper's health `up` message is unchanged.
    # A restart would have refreshed its `time` (that is the SIGUSR1 behaviour),
    # while its `pid` is the supervisor process id either way.
    Mapper Not Restarted Since    ${before}

Explicit override is not affected by SIGHUP
    [Documentation]    With RUST_LOG in effect the reload handle is disabled, so SIGHUP is
    ...    ignored and the pinned levels stay put.
    Start Supervisor With Env    RUST_LOG=info
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    # Raising the mapper in system.toml and signalling a reload must be a no-op: the
    # supervisor logs that the levels are fixed and leaves them alone.
    Set Component Log Level    tedge-mapper-c8y    debug
    ${start}=    Get Unix Timestamp
    Execute Command    cmd=systemctl kill --signal=SIGHUP tedge-run-all.service
    Wait Until Keyword Succeeds
    ...    30s    2s    Supervisor Log Should Contain    log levels are fixed    date_from=${start}

    # The level really did not change: a published measurement is still dropped
    # because the mapper stays at the RUST_LOG-pinned INFO level.
    ${after}=    Get Unix Timestamp
    Trigger Mapper Mqtt Activity
    Sleep    2s    reason=Give the mapper time to (not) log the received message
    Supervisor Log Should Not Contain    MQTT recv    date_from=${after}


*** Keywords ***
Register Device
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}

    # Hand the components over to the supervisor: stop the systemd-managed services so
    # their single-instance locks stay free for `tedge run all`.
    Execute Command    systemctl stop tedge-mapper-c8y tedge-agent

    # Keep a pristine copy of system.toml so each test starts from the same levels and
    # `Set Component Log Level` never stacks duplicate `[log]` tables across tests.
    Execute Command    cmd=cp /etc/tedge/system.toml /etc/tedge/system.toml.orig    ignore_exit_code=${True}

Start Supervisor
    # `tedge run all` is a long-running foreground process, so launch it as a transient
    # systemd unit (running as the tedge user, like the real services). `--collect` reaps
    # the unit when it stops, freeing the name for the next test.
    Execute Command
    ...    cmd=systemd-run --unit=tedge-run-all --collect -p User=tedge -p Group=tedge /usr/bin/tedge run all c8y

Start Supervisor With Env
    [Arguments]    ${env}
    # Same as `Start Supervisor` but with an extra environment variable in the unit, used
    # to put an explicit log override (e.g. RUST_LOG) in effect for the override test.
    Execute Command
    ...    cmd=systemd-run --unit=tedge-run-all --collect -p User=tedge -p Group=tedge -E ${env} /usr/bin/tedge run all c8y

Set Component Log Level
    [Arguments]    ${component}    ${level}
    # Rewrite system.toml from the pristine copy plus a fresh `[log]` table, so repeated
    # calls never accumulate stale entries or duplicate sections.
    Execute Command    cmd=cp -f /etc/tedge/system.toml.orig /etc/tedge/system.toml    ignore_exit_code=${True}
    Execute Command    cmd=printf '\\n[log]\\n%s = "%s"\\n' '${component}' '${level}' >> /etc/tedge/system.toml

Trigger Mapper Mqtt Activity
    # The c8y mapper subscribes to the `te/#` topic tree, so a published measurement is
    # received by it and, at DEBUG, logged with the `MQTT recv` target.
    Execute Command    cmd=tedge mqtt pub te/device/main///m/test '{"temperature":25}'

Supervisor Log Should Contain
    [Arguments]    ${pattern}    ${date_from}=${EMPTY}
    ${logs}=    Get Supervisor Logs    ${date_from}
    Should Contain    ${logs}    ${pattern}

Supervisor Log Should Not Contain
    [Arguments]    ${pattern}    ${date_from}=${EMPTY}
    ${logs}=    Get Supervisor Logs    ${date_from}
    Should Not Contain    ${logs}    ${pattern}

Get Supervisor Logs
    [Arguments]    ${date_from}=${EMPTY}
    # Read the supervisor unit's journal, optionally only since the given unix timestamp
    # so a check is scoped to events after a particular point in the test.
    IF    "${date_from}" == "${EMPTY}"
        ${logs}=    Execute Command    cmd=journalctl -u tedge-run-all.service --no-pager --output=cat
    ELSE
        ${logs}=    Execute Command
        ...    cmd=journalctl -u tedge-run-all.service --no-pager --output=cat --since @${date_from}
    END
    RETURN    ${logs}

Mapper Not Restarted Since
    [Arguments]    ${before}
    # SIGHUP must not restart the mapper: its health `up` message keeps the same `time`
    # (a restart would refresh it — that is the SIGUSR1 behaviour) and the same `pid`
    # (the supervisor process id, unchanged across the whole test).
    ${now}=    Service Health Status Should Be Up    tedge-mapper-c8y
    Should Be Equal As Numbers    ${now["time"]}    ${before["time"]}
    Should Be Equal As Integers    ${now["pid"]}    ${before["pid"]}

Stop Supervisor
    # Tear the supervisor down and restore the pristine system.toml so the next test starts
    # from the same levels. The device registration from the suite setup is left untouched.
    Execute Command    systemctl stop tedge-run-all.service    ignore_exit_code=${True}
    Execute Command    cmd=cp -f /etc/tedge/system.toml.orig /etc/tedge/system.toml    ignore_exit_code=${True}
    Get Logs
