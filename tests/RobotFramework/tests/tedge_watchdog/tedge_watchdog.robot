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
${WATCHDOG_SEC}     5
${SERVICE_NAME}     tedge-agent


*** Test Cases ***
# NOTE: This test succeeds, but not for the reason we expect - instead of simply not sending a notification, upon
# reciving a status:down message tedge-watchdog exits for unknown reason, so it can't send the notification and the
# monitored service is restarted by systemd, along with tedge-watchdog itself.
# This is a result of a specific configuration of these services' unit files, that we're not checking or validating
# anywhere, so we need to define this behaviour more precisely by providing additional tests.
Watchdog doesn't send notification if service is down which leads to service restart
    Set Service health check response    respond=0
    Restart Service    ${SERVICE_NAME}
    Restart Service    tedge-watchdog

    # wait for service to start
    ${pid} =    Service Should Be Running    ${SERVICE_NAME}

    # send status up
    ${ts} =    Get Unix Timestamp

    # wait for a health check command from watchdog
    Should Have MQTT Messages    te/device/main/service/${SERVICE_NAME}/cmd/health/check

    Sleep    ${WATCHDOG_SEC}s

    # Verify that service got restarted by systemd and it's now healthy
    ${pid1} =    Service Should Be Running    ${SERVICE_NAME}
    Should Not Be Equal    ${pid}    ${pid1}

Watchdog doesn't fail on unexpected time format
    Set Service health check response    respond=0
    Restart Service    ${SERVICE_NAME}
    Restart Service    tedge-watchdog

    # wait for service to start
    ${pid_watchdog} =    Service Should Be Running    tedge-watchdog
    ${pid_service} =    Service Should Be Running    ${SERVICE_NAME}

    # send status up
    ${ts} =    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'te/device/main/service/${SERVICE_NAME}/status/health' '{"status": "down", "pid": ${pid_service}, "time": ${ts}}'

    # wait for a health check command from watchdog
    Should Have MQTT Messages    te/device/main/service/${SERVICE_NAME}/cmd/health/check

    # Simulate service signalling that it's unhealthy with wrong format in `time` field
    # (we're expecting a UNIX timestamp to be an int, not a string)
    ${before_restart_timestamp} =    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'te/device/main/service/${SERVICE_NAME}/status/health' '{"status": "down", "pid": ${pid_service}, "time": "${before_restart_timestamp}"}'

    # Wait until service is restarted due to tedge-watchdog not sending out the notification
    Sleep    ${WATCHDOG_SEC}s

    # Verify that tedge-watchdog is still running
    ${pid_watchdog1} =    Service Should Be Running    tedge-watchdog
    ${pid_service1} =    Service Should Be Running    ${SERVICE_NAME}

    Should Be Equal    ${pid_watchdog}    ${pid_watchdog1}
    Should Not Be Equal    ${pid_service}    ${pid_service1}

Watchdog sends notification if service is up which leads to service continuing to run
    Set Service health check response    respond=1

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

    # Need to manually restart the mqtt-logger because after --no-bootstrap --no-connect step it
    # isn't restarted, whereas normal bootstrap would have restarted it
    # TODO: update the bootstrap logic to always restart mqtt-logger
    Restart Service    mqtt-logger

    Execute Command    mv /lib/systemd/system/tedge-agent.service /lib/systemd/system/tedge-agent.service.bak
    Transfer To Device    ${CURDIR}/tedge-agent.service    /lib/systemd/system/

    Execute Command    cmd=sed -i '13iWatchdogSec=${WATCHDOG_SEC}' /lib/systemd/system/${service_name}.service

    Transfer To Device    ${CURDIR}/health_check_respond.sh    /setup/

    Restart Service    tedge-watchdog

    Restart Service    ${SERVICE_NAME}

Custom Teardown
    Execute Command    rm /setup/health_check_respond.sh
    Execute Command    mv /lib/systemd/system/tedge-agent.service.bak /lib/systemd/system/tedge-agent.service
    Get Logs

Set Service health check response
    [Arguments]    ${respond}
    Execute Command
    ...    sed -i 's/Environment\=RESPOND\=.*/Environment\=RESPOND\=${respond}/' /lib/systemd/system/${SERVICE_NAME}.service
    Execute Command    systemctl daemon-reload
    Execute Command    systemctl restart ${SERVICE_NAME}
