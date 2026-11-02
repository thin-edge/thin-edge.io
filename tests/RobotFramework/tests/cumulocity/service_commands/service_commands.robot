*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:services    theme:operation


*** Variables ***
${DEVICE_SN}        ${EMPTY}
${SERVICE_NAME}     dummy-service
${SERVICE_XID}      ${EMPTY}
${AGENT_XID}        ${EMPTY}
${MAPPER_XID}       ${EMPTY}


*** Test Cases ***
Declares and removes an action of a service
    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/${SERVICE_NAME}/cmd/control '{"actions":["restart","pause"]}'
    Supported Service Commands Should Be    ${SERVICE_XID}    restart    pause
    Cumulocity.Should Contain Supported Operations    c8y_ServiceCommand
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/c8y/${SERVICE_XID}/c8y_ServiceCommand

    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/${SERVICE_NAME}/cmd/control '{"actions":["restart"]}'
    Supported Service Commands Should Be    ${SERVICE_XID}    restart

Declares the action of thin-edge's own services
    Supported Service Commands Should Be    ${AGENT_XID}    restart
    Cumulocity.Should Contain Supported Operations    c8y_ServiceCommand

    Supported Service Commands Should Be    ${MAPPER_XID}    restart
    Cumulocity.Should Contain Supported Operations    c8y_ServiceCommand

Restarts a service managed by the init system
    ${before}=    Get Service PID    ${SERVICE_NAME}
    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"restart","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ${after}=    Get Service PID    ${SERVICE_NAME}
    Should Not Be Equal    ${before}    ${after}    The service was not restarted

Rejects an action the service has not declared
    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"Collect_Measurements","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*has not declared the 'Collect_Measurements' action.*
    ...    timeout=60

Restarts tedge-agent itself
    ${before}=    Get Service PID    tedge-agent

    ${operation}=    Create Service Command Operation
    ...    ${AGENT_XID}
    ...    {"command":"restart","serviceName":"tedge-agent","serviceType":"service"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=180

    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
    ${after}=    Get Service PID    tedge-agent
    Should Not Be Equal    ${before}    ${after}    tedge-agent was not restarted


*** Keywords ***
Custom Setup
    ${sn}=    Setup
    Set Suite Variable    $DEVICE_SN    ${sn}
    Set Suite Variable    $SERVICE_XID    ${sn}:device:main:service:${SERVICE_NAME}
    Set Suite Variable    $AGENT_XID    ${sn}:device:main:service:tedge-agent
    Set Suite Variable    $MAPPER_XID    ${sn}:device:main:service:tedge-mapper-c8y
    Cumulocity.External Identity Should Exist    ${sn}    show_info=${False}

    # The service is registered under its systemd unit name.
    # Cumulocity sends this name back in the operation, and the workflow passes it to systemctl.
    ThinEdgeIO.Transfer To Device    ${CURDIR}/dummy-service.service    /etc/systemd/system/
    Execute Command    systemctl daemon-reload && systemctl start ${SERVICE_NAME}

    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/${SERVICE_NAME} '{"name":"${SERVICE_NAME}","@type":"service"}'
    Cumulocity.External Identity Should Exist    ${SERVICE_XID}    show_info=${False}
    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/${SERVICE_NAME}/cmd/control '{"actions":["restart"]}'

Supported Service Commands Should Be
    [Arguments]    ${external_id}    @{expected}
    Cumulocity.External Identity Should Exist    ${external_id}    show_info=${False}
    Wait Until Keyword Succeeds    30x    2s    Managed Object Service Commands Should Be    @{expected}

Managed Object Service Commands Should Be
    [Arguments]    @{expected}
    ${mo}=    Cumulocity.Managed Object Should Have Fragments    c8y_SupportedServiceCommands
    ${actual}=    Evaluate    $mo["c8y_SupportedServiceCommands"]
    Should Be Equal    ${actual}    ${expected}

Create Service Command Operation
    [Arguments]    ${external_id}    ${fragment}
    Cumulocity.External Identity Should Exist    ${external_id}    show_info=${False}
    ${operation}=    Cumulocity.Create Operation
    ...    fragments={"c8y_ServiceCommand":${fragment}}
    ...    description=Service command
    RETURN    ${operation}
