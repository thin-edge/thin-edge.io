*** Settings ***
Resource            ../../../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_agent    theme:services


*** Test Cases ***
thin-edge services declare their restart action
    ${capabilities}=    Execute Command
    ...    tedge mqtt sub te/device/main/service/+/cmd/restart --retained-only --duration 2s
    Should Contain    ${capabilities}    [te/device/main/service/tedge-agent/cmd/restart] {}
    Should Contain    ${capabilities}    [te/device/main/service/tedge-mapper-c8y/cmd/restart] {}

Only the workflow for type device declares a capability
    ${capabilities}=    Execute Command    tedge mqtt sub te/+/+/+/+/cmd/probe --retained-only --duration 2s
    Should Contain    ${capabilities}    [te/device/main///cmd/probe] {}
    Should Not Contain    ${capabilities}    te/device/main/service/sleeper/cmd/probe

Agent restarts a service of its own device
    ${pid_before}=    Get Service PID    sleeper
    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/sleeper/cmd/restart/robot-1 '{"status":"init","serviceName":"sleeper","serviceType":"service"}'
    Should Have MQTT Messages
    ...    te/device/main/service/sleeper/cmd/restart/robot-1
    ...    message_pattern=.*successful.*
    ...    maximum=1
    ${pid_after}=    Get Service PID    sleeper
    Should Not Be Equal    ${pid_before}    ${pid_after}
    Execute Command    tedge mqtt pub --retain te/device/main/service/sleeper/cmd/restart/robot-1 ''

A command targeting a service runs a service workflow
    Execute Command    tedge mqtt pub --retain te/device/main/service/sleeper/cmd/probe/robot-2 '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/main/service/sleeper/cmd/probe/robot-2
    ...    message_pattern=.*service-probed.*
    ...    maximum=1
    Execute Command    tedge mqtt pub --retain te/device/main/service/sleeper/cmd/probe/robot-2 ''

A command targeting a device runs a device workflow
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/probe/robot-3 '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/probe/robot-3
    ...    message_pattern=.*device-probed.*
    ...    maximum=1
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/probe/robot-3 ''

A service restart workflow can restart tedge-agent
    ${pid_before}=    Get Service PID    tedge-agent
    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/tedge-agent/cmd/restart/robot-4 '{"status":"init","serviceName":"tedge-agent","serviceType":"service"}'
    Should Have MQTT Messages
    ...    te/device/main/service/tedge-agent/cmd/restart/robot-4
    ...    message_pattern=.*successful.*
    ...    maximum=1
    ...    timeout=120
    ${pid_after}=    Get Service PID    tedge-agent
    Should Not Be Equal    ${pid_before}    ${pid_after}
    Execute Command    tedge mqtt pub --retain te/device/main/service/tedge-agent/cmd/restart/robot-4 ''


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN

    Transfer To Device    ${CURDIR}/sleeper.service    /etc/systemd/system/
    Execute Command    systemctl daemon-reload && systemctl start sleeper

    Transfer To Device    ${CURDIR}/probe.toml    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/service_probe.toml    /etc/tedge/operations/

    Register Entity    device/main/service/sleeper    service    device/main//
