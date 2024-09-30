*** Settings ***
Documentation
...                 Test suite for tedge-watchdog service.
...
...                 Suite needs to check behaviour of tedge-watchdog when services respond in various ways in response
...                 to watchdog-sent health check commands. Watchdog can check only a statically enumerated set of tedge
...                 services, so we need to take some tedge service and override its default health check responses to
...                 test the watchdog.
...
...                 Reference: docs/src/operate/monitoring/systemd-watchdog.md

Resource            ../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           adapter:docker    theme:tedge-watchdog


*** Variables ***
# Interval at which we send notifications/check health of the service. Use smaller value for test to complete faster but
# increase if flaky or debugging.
${WATCHDOG_SEC}     60
${SERVICE_NAME}     tedge-agent


*** Test Cases ***
# NOTE: This test succeeds, but not for the reason we expect - instead of simply not sending a notification, upon
# reciving a status:down message tedge-watchdog exits for unknown reason, so it can't send the notification and the
# monitored service is restarted by systemd, along with tedge-watchdog itself.
# This is a result of a specific configuration of these services' unit files, that we're not checking or validating
# anywhere, so we need to define this behaviour more precisely by providing additional tests.
Watchdog doesn't send notification if service is down which leads to service restart
    # wait for service to start
    ${pid} =    Service Should Be Running    ${SERVICE_NAME}

    # send status up
    ${ts} =    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'te/device/main/service/${SERVICE_NAME}/status/health' '{"status": "down", "pid": ${pid}, "time": ${ts}}'

    # wait for a health check command from watchdog
    Should Have MQTT Messages    te/device/main/service/${SERVICE_NAME}/cmd/health/check

    # Simulate service signalling that it's unhealthy
    ${before_restart_timestamp} =    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'te/device/main/service/${SERVICE_NAME}/status/health' '{"status": "down", "pid": ${pid}, "time": ${before_restart_timestamp}}'

    # Wait until service is restarted due to tedge-watchdog not sending out the notification
    Sleep    ${WATCHDOG_SEC}s

    # Verify that service got restarted by systemd and it's now healthy
    ${pid1} =    Service Should Be Running    ${SERVICE_NAME}
    Should Not Be Equal    ${pid}    ${pid1}

    Should Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/status/health
    ...    message_contains="status":"up"
    ...    date_from=${before_restart_timestamp}

Watchdog doesn't fail on unexpected time format
    # wait for service to start
    ${pid} =    Service Should Be Running    tedge-watchdog

    # send status up
    ${ts} =    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'te/device/main/service/${SERVICE_NAME}/status/health' '{"status": "down", "pid": ${pid}, "time": ${ts}}'

    # wait for a health check command from watchdog
    Should Have MQTT Messages    te/device/main/service/${SERVICE_NAME}/cmd/health/check

    # Simulate service signalling that it's unhealthy with wrong format in `time` field
    # (we're expecting a UNIX timestamp to be an int, not a string)
    ${before_restart_timestamp} =    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'te/device/main/service/${SERVICE_NAME}/status/health' '{"status": "down", "pid": ${pid}, "time": "${before_restart_timestamp}"}'

    # Wait until service is restarted due to tedge-watchdog not sending out the notification
    Sleep    ${WATCHDOG_SEC}s

    # Verify that tedge-watchdog is still running
    ${pid1} =    Service Should Be Running    tedge-watchdog
    Should Be Equal    ${pid}    ${pid1}

Watchdog sends notification if service is up which leads to service continuing to run
    # AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
    # Set Service health check response    {\\\"status\\\":\\\"up\\\", \\\"pid\\\":\\\"$$\\\"}
    Restart Service    ${SERVICE_NAME}
    Restart Service    tedge-watchdog

    ${pid} =    Service Should Be Running    ${SERVICE_NAME}

    # send status up
    ${ts} =    Get Unix Timestamp

    # wait for a health check command from watchdog
    Should Have MQTT Messages    te/device/main/service/${SERVICE_NAME}/cmd/health/check    date_from=${ts}

    # Wait until we're sure systemd sees the tedge-watchdog notification about service being healthy
    Sleep    ${WATCHDOG_SEC}s

    # Verify that service wasn't restarted by systemd
    ${pid1} =    Service Should Be Running    ${SERVICE_NAME}
    Should Be Equal    ${pid}    ${pid1}


*** Keywords ***
Custom Setup
    Setup    skip_bootstrap=True
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect

    Execute Command    mv /lib/systemd/system/tedge-agent.service /lib/systemd/system/tedge-agent.service.bak
    Transfer To Device    ${CURDIR}/tedge-agent.service    /lib/systemd/system/

    Execute Command    cmd=sed -i '13iWatchdogSec=${WATCHDOG_SEC}' /lib/systemd/system/${service_name}.service

    Transfer To Device    ${CURDIR}/health_check_respond.sh    /setup/

    # Without this line mqtt-logger can't connect to listener at 1883, but with it it successfully connects to listener
    # at 8883
    Restart Service    mqtt-logger

    Restart Service    tedge-watchdog

    Restart Service    ${SERVICE_NAME}

Custom Teardown
    Execute Command    rm /setup/health_check_respond.sh
    Execute Command    mv /lib/systemd/system/tedge-agent.service.bak /lib/systemd/system/tedge-agent.service
    Get Logs

Set Service health check response
    [Arguments]    ${message}
    Execute Command    sed -i 's/Environment\=RESPONSE\=.*/Environment\="RESPONSE\=${message}"/' /lib/systemd/system/${SERVICE_NAME}.service
    Execute Command    systemctl daemon-reload
