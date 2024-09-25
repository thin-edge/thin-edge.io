*** Settings ***
Documentation
...                 Test suite for tedge-watchdog service.
...
...                 Reference: docs/src/operate/monitoring/systemd-watchdog.md

Resource            ../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           adapter:docker    theme:tedge-watchdog


*** Variables ***
# Interval at which we send notifications/check health of the service. Use smaller value for test to complete faster but
# increase if flaky.
${WATCHDOG_SEC}     5
${SERVICE_NAME}     tedge-agent


*** Test Cases ***
# NOTE: This test succeeds, but not for the reason we expect - instead of simply not sending a notification, upon
# reciving a status:down message tedge-watchdog exits for unknown reason, so it can't send the notification and the
# monitored service is restarted by systemd, along with tedge-watchdog itself.
# This is a result of a specific configuration of these services' unit files, that we're not checking or validating
# anywhere, so we need to define this behaviour more precisely by providing additional tests.
Watchdog doesn't send notification if service is down which leads to service restart
    Start Service    ${SERVICE_NAME}
    Start Service    tedge-watchdog

    # wait for service to start
    ${pid} =    Service Should Be Running    ${SERVICE_NAME}

    # Simulate service signalling that it's unhealthy
    ${before_restart_timestamp} =    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'te/device/main/service/${SERVICE_NAME}/status/health' '{"status": "down", "pid": ${pid}, "time": "${before_restart_timestamp}"}'

    # Wait until service is restarted due to tedge-watchdog not sending out the notification
    Sleep    ${WATCHDOG_SEC}s

    # Verify that service got restarted by systemd and it's now healthy
    ${pid1} =    Service Should Be Running    ${SERVICE_NAME}
    Should Not Be Equal    ${pid}    ${pid1}

    Should Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/status/health
    ...    message_contains="status":"up"
    ...    date_from=${before_restart_timestamp}

Watchdog sends notification if service is up which leads to service continuing to run
    Start Service    ${SERVICE_NAME}
    Start Service    tedge-watchdog

    ${pid} =    Service Should Be Running    ${SERVICE_NAME}

    # wait for service to publish up message
    Sleep    2s

    # Make sure service is healthy
    Should Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/status/health
    ...    message_contains="status":"up"

    # Wait until we're sure systemd sees the tedge-watchdog notification about service being healthy
    Sleep    ${WATCHDOG_SEC}s

    # Verify that service wasn't restarted by systemd
    ${pid1} =    Service Should Be Running    ${SERVICE_NAME}
    Should Be Equal    ${pid}    ${pid1}

    Should Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/status/health
    ...    message_contains="status":"up"

*** Keywords ***
Enable watchdog for service
    [Documentation]    Edits systemd unit file for a given service so it can be run with tedge-watchdog.
    ...    Provide only service name, e.g. `tedge-mapper-c8y`.
    [Arguments]    ${service_name}

    # inserts `WatchdogSec=...` line at a given position. Ideally we should use `systemctl --edit` to not overwrite the
    # default unit file provided by the package but it's not currently supported by tedge-watchdog.
    Execute Command    cmd=sudo sed -i '11iWatchdogSec=${WATCHDOG_SEC}' /lib/systemd/system/${service_name}.service

    # will reload affected services if they're running
    Execute Command    sudo systemctl daemon-reload

Disable watchdog for service
    [Documentation]    Edits systemd unit file for a given service so it can be run with tedge-watchdog.
    ...    Provide only service name, e.g. `tedge-mapper-c8y`.
    [Arguments]    ${service_name}

    # inserts `WatchdogSec=...` line at a given position. Ideally we should use `systemctl --edit` to not overwrite the
    # default unit file provided by the package but it's not currently supported by tedge-watchdog.
    Execute Command    cmd=sudo sed -i '11d' /lib/systemd/system/${service_name}.service

    # will reload affected services if they're running
    Execute Command    sudo systemctl daemon-reload

Custom Setup
    Setup    skip_bootstrap=True
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect

    # Without this line mqtt-logger can't connect to listener at 1883, but with it it successfully connects to listener
    # at 8883
    Restart Service    mqtt-logger

    Enable watchdog for service    ${SERVICE_NAME}

Custom Teardown
    Disable watchdog for service    ${SERVICE_NAME}
    Get Logs
