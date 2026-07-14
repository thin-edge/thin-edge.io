*** Settings ***
Documentation       Signal handling for standalone tedge-agent and tedge-mapper.
...                 On SIGHUP each standalone process re-reads its `[log]` level from
...                 `system.toml` and applies it without restarting, unless an explicit
...                 override (`--log-level`/`--debug`/`RUST_LOG`) is in effect.
...                 The change is proven from each service's journal: the agent's entity
...                 store logs the requests it serves at DEBUG, so an HTTP query leaves an
...                 `EntityStoreServer: recv` line only once the agent has been raised to
...                 `debug`; likewise the mapper's MQTT actor leaves an `MQTT recv` line
...                 for a published measurement.
...
...                 SIGUSR1 is the mapper-restart signal used by the `tedge run all`
...                 supervisor. A standalone mapper must ignore it so that a stray signal
...                 does not disrupt the service.

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Register Device
Test Teardown       Restore Log Levels

Test Tags           theme:supervisor    theme:c8y


*** Test Cases ***
Agent applies updated log levels on SIGHUP without restarting
    ${before}=    Service Health Status Should Be Up    tedge-agent

    # Baseline: at the default INFO level the agent does not log the HTTP requests it
    # serves, so querying the entity store leaves no `EntityStoreServer: recv` line.
    ${start}=    Get Unix Timestamp
    Query Entity Store
    Sleep    2s    reason=Give the agent time to (not) log the served request
    Service Log Should Not Contain    tedge-agent    EntityStoreServer: recv    date_from=${start}

    # Operator raises the agent to debug and signals a reload.
    Set Component Log Level    tedge-agent    debug
    Execute Command    cmd=systemctl kill --signal=SIGHUP tedge-agent
    Wait Until Keyword Succeeds
    ...    30s    2s    Service Log Should Contain    tedge-agent    log levels reloaded    date_from=${start}

    # After the reload the same HTTP query is now recorded at DEBUG, proving the new
    # level applies to subsequent records without a restart.
    ${after}=    Get Unix Timestamp
    Query Entity Store
    Wait Until Keyword Succeeds
    ...    30s    2s    Service Log Should Contain    tedge-agent    EntityStoreServer: recv    date_from=${after}

    Service Not Restarted Since    tedge-agent    ${before}

Mapper applies updated log levels on SIGHUP without restarting
    ${before}=    Service Health Status Should Be Up    tedge-mapper-c8y

    # Baseline: at the default INFO level the mapper does not log the messages it
    # receives, so a published measurement leaves no `MQTT recv` line behind.
    ${start}=    Get Unix Timestamp
    Trigger Mapper Mqtt Activity
    Sleep    2s    reason=Give the mapper time to (not) log the received message
    Service Log Should Not Contain    tedge-mapper-c8y    MQTT recv    date_from=${start}

    # Operator raises the mapper to debug and signals a reload.
    Set Component Log Level    tedge-mapper-c8y    debug
    Execute Command    cmd=systemctl kill --signal=SIGHUP tedge-mapper-c8y
    Wait Until Keyword Succeeds
    ...    30s    2s    Service Log Should Contain    tedge-mapper-c8y    log levels reloaded    date_from=${start}

    # After the reload the same publish is now recorded at DEBUG, proving the new
    # level applies to subsequent records without a restart.
    ${after}=    Get Unix Timestamp
    Trigger Mapper Mqtt Activity
    Wait Until Keyword Succeeds
    ...    30s    2s    Service Log Should Contain    tedge-mapper-c8y    MQTT recv    date_from=${after}

    Service Not Restarted Since    tedge-mapper-c8y    ${before}

SIGUSR1 does not restart the standalone mapper
    ${before}=    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    Execute Command    cmd=systemctl kill --signal=SIGUSR1 tedge-mapper-c8y

    Sleep    5s    reason=Give the mapper time to (not) react to SIGUSR1
    Service Not Restarted Since    tedge-mapper-c8y    ${before}

Explicit override is not affected by SIGHUP
    [Documentation]    With RUST_LOG in effect the reload handle is disabled, so SIGHUP is
    ...    ignored and the pinned levels stay put.
    Pin Agent Log Level With RUST_LOG    info
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent

    # Raising the agent in system.toml and signalling a reload must be a no-op: the
    # agent logs that the levels are fixed and leaves them alone.
    Set Component Log Level    tedge-agent    debug
    ${start}=    Get Unix Timestamp
    Execute Command    cmd=systemctl kill --signal=SIGHUP tedge-agent
    Wait Until Keyword Succeeds
    ...    30s    2s    Service Log Should Contain    tedge-agent    log levels are fixed    date_from=${start}

    # The level really did not change: an HTTP query is still dropped because the
    # agent stays at the RUST_LOG-pinned INFO level.
    ${after}=    Get Unix Timestamp
    Query Entity Store
    Sleep    2s    reason=Give the agent time to (not) log the served request
    Service Log Should Not Contain    tedge-agent    EntityStoreServer: recv    date_from=${after}
    [Teardown]    Remove Log Override


*** Keywords ***
Register Device
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}

    # Keep a pristine copy of system.toml so each test starts from the same levels and
    # `Set Component Log Level` never stacks duplicate `[log]` tables across tests.
    Execute Command    cmd=cp /etc/tedge/system.toml /etc/tedge/system.toml.orig    ignore_exit_code=${True}

Set Component Log Level
    [Arguments]    ${component}    ${level}
    # Rewrite system.toml from the pristine copy plus a fresh `[log]` table, so repeated
    # calls never accumulate stale entries or duplicate sections.
    Execute Command    cmd=cp -f /etc/tedge/system.toml.orig /etc/tedge/system.toml    ignore_exit_code=${True}
    Execute Command    cmd=printf '\\n[log]\\n%s = "%s"\\n' '${component}' '${level}' >> /etc/tedge/system.toml

Query Entity Store
    # The agent serves the entity store over its HTTP API and, at DEBUG, logs each
    # served request with the `EntityStoreServer` target.
    Execute Command    cmd=tedge http get /te/v1/entities

Trigger Mapper Mqtt Activity
    # The c8y mapper subscribes to the `te/#` topic tree, so a published measurement is
    # received by it and, at DEBUG, logged with the `MQTT recv` target.
    Execute Command    cmd=tedge mqtt pub te/device/main///m/test '{"temperature":25}'

Pin Agent Log Level With RUST_LOG
    [Arguments]    ${level}
    # Put an explicit log override in effect for the agent service: with RUST_LOG set,
    # log levels are pinned for the process lifetime and the reload handle is disabled.
    Execute Command    cmd=mkdir -p /etc/systemd/system/tedge-agent.service.d
    Execute Command
    ...    cmd=printf '[Service]\\nEnvironment=RUST_LOG=%s\\n' '${level}' > /etc/systemd/system/tedge-agent.service.d/10-rust-log.conf
    Execute Command    cmd=systemctl daemon-reload && systemctl restart tedge-agent

Remove Log Override
    # Drop the RUST_LOG override and restart the agent back onto file-configured levels.
    Execute Command    cmd=rm -f /etc/systemd/system/tedge-agent.service.d/10-rust-log.conf
    Execute Command    cmd=systemctl daemon-reload && systemctl restart tedge-agent
    Restore Log Levels

Service Log Should Contain
    [Arguments]    ${service}    ${pattern}    ${date_from}=${EMPTY}
    ${logs}=    Get Service Logs    ${service}    ${date_from}
    Should Contain    ${logs}    ${pattern}

Service Log Should Not Contain
    [Arguments]    ${service}    ${pattern}    ${date_from}=${EMPTY}
    ${logs}=    Get Service Logs    ${service}    ${date_from}
    Should Not Contain    ${logs}    ${pattern}

Get Service Logs
    [Arguments]    ${service}    ${date_from}=${EMPTY}
    # Read the service unit's journal, optionally only since the given unix timestamp
    # so a check is scoped to events after a particular point in the test.
    IF    "${date_from}" == "${EMPTY}"
        ${logs}=    Execute Command    cmd=journalctl -u ${service} --no-pager --output=cat
    ELSE
        ${logs}=    Execute Command
        ...    cmd=journalctl -u ${service} --no-pager --output=cat --since @${date_from}
    END
    RETURN    ${logs}

Service Not Restarted Since
    [Arguments]    ${service}    ${before}
    ${now}=    Service Health Status Should Be Up    ${service}
    Should Be Equal As Numbers
    ...    ${now["time"]}    ${before["time"]}
    ...    msg=${service} was restarted (health timestamp changed from ${before["time"]} to ${now["time"]})
    Should Be Equal As Integers
    ...    ${now["pid"]}    ${before["pid"]}
    ...    msg=${service} was restarted (PID changed from ${before["pid"]} to ${now["pid"]})

Restore Log Levels
    # Restore the pristine system.toml and re-signal both services so the next test
    # starts from the default levels. The device registration is left untouched.
    Execute Command    cmd=cp -f /etc/tedge/system.toml.orig /etc/tedge/system.toml    ignore_exit_code=${True}
    Execute Command    cmd=systemctl kill --signal=SIGHUP tedge-agent tedge-mapper-c8y    ignore_exit_code=${True}
    Get Logs
