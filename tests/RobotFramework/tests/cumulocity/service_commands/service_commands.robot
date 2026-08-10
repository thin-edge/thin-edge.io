*** Settings ***
Documentation       Service commands run through the init system, the backend of the default
...                 service type. Cumulocity declares the actions of a service with
...                 c8y_SupportedServiceCommands and triggers them with c8y_ServiceCommand.

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
Declares the actions of a service to Cumulocity
    Cumulocity.External Identity Should Exist    ${SERVICE_XID}    show_info=${False}
    Cumulocity.Should Contain Supported Operations    c8y_ServiceCommand
    Supported Service Commands Should Be    ${SERVICE_XID}    RESTART    START    STOP

Declares the actions of thin-edge's own services
    [Documentation]    tedge-agent and every cloud mapper declare their own actions when they
    ...    start, with nobody asking for it. Neither declares STOP or START: the shipped
    ...    workflow always refuses to stop them, so the action is never offered.
    Supported Service Commands Should Be    ${AGENT_XID}    RESTART    ENABLE    DISABLE
    Cumulocity.Should Contain Supported Operations    c8y_ServiceCommand

    Supported Service Commands Should Be    ${MAPPER_XID}    RESTART    ENABLE    DISABLE
    Cumulocity.Should Contain Supported Operations    c8y_ServiceCommand

Declares every action of a mapper connected to no cloud
    [Documentation]    The collectd mapper and the local mapper are connected to no cloud, so
    ...    stopping one takes no way of reporting anything away and both declare all five
    ...    actions. Neither runs by default, so this test starts them and stops them again.
    Execute Command    systemctl start tedge-mapper-collectd tedge-mapper-local

    Supported Service Commands Should Be
    ...    ${DEVICE_SN}:device:main:service:tedge-mapper-collectd
    ...    DISABLE    ENABLE    RESTART    START    STOP
    Supported Service Commands Should Be
    ...    ${DEVICE_SN}:device:main:service:tedge-mapper-local
    ...    DISABLE    ENABLE    RESTART    START    STOP
    [Teardown]    Stop The Mappers Connected To No Cloud

Withdraws an action from Cumulocity
    Declare Action    ${SERVICE_NAME}    pause
    Supported Service Commands Should Be    ${SERVICE_XID}    PAUSE    RESTART    START    STOP

    Withdraw Action    ${SERVICE_NAME}    pause
    Supported Service Commands Should Be    ${SERVICE_XID}    RESTART    START    STOP

Restarts a service managed by the init system
    ${before}=    Get Service PID    ${SERVICE_NAME}
    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"RESTART","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ${after}=    Get Service PID    ${SERVICE_NAME}
    Should Not Be Equal    ${before}    ${after}    The service was not restarted

Stops and starts a service managed by the init system
    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"STOP","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Service Should Be Stopped    ${SERVICE_NAME}

    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"START","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Service Should Be Running    ${SERVICE_NAME}

Enables and disables a service managed by the init system
    Declare Action    ${SERVICE_NAME}    enable
    Declare Action    ${SERVICE_NAME}    disable
    Supported Service Commands Should Be
    ...    ${SERVICE_XID}    DISABLE    ENABLE    RESTART    START    STOP

    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"ENABLE","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Service Should Be Enabled    ${SERVICE_NAME}

    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"DISABLE","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Service Should Be Disabled    ${SERVICE_NAME}

Rejects an action the service has not declared
    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"COLLECT_MEASUREMENTS","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*has not declared the 'collect_measurements' action.*
    ...    timeout=60
    Should Not Have MQTT Messages    te/device/main/service/${SERVICE_NAME}/cmd/collect_measurements/+

Rejects a command that is not a valid action name
    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"Do Something","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*cannot run the command 'Do Something'.*
    ...    timeout=60

Rejects a service name a backend could misread
    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"RESTART","serviceName":"--now","serviceType":"service"}
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*Invalid service name '--now'.*
    ...    timeout=60

    ${operation_id}=    Set Variable    ${operation.to_json()["id"]}
    Should Not Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/cmd/restart/c8y-mapper-${operation_id}

Rejects a service type that names no plugin file
    ${operation}=    Create Service Command Operation
    ...    ${SERVICE_XID}
    ...    {"command":"RESTART","serviceName":"${SERVICE_NAME}","serviceType":"../../bin/sh"}
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*Invalid service type '../../bin/sh'.*
    ...    timeout=60

    ${operation_id}=    Set Variable    ${operation.to_json()["id"]}
    Should Not Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/cmd/restart/c8y-mapper-${operation_id}

Refuses to stop tedge-agent
    # The agent does not declare `stop`, so the action has to be declared by hand to reach the
    # guard which refuses it. That guard is what enforces the refusal: a capability is a retained
    # message anyone can publish, and the declaration only says what is offered.
    Declare Action    tedge-agent    stop
    ${operation}=    Create Service Command Operation
    ...    ${AGENT_XID}
    ...    {"command":"STOP","serviceName":"tedge-agent","serviceType":"service"}
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*cannot stop itself.*
    ...    timeout=120
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Refuses to stop a mapper connected to a cloud
    # Declared by hand for the same reason as above: a cloud mapper offers no `stop` either.
    Declare Action    tedge-mapper-c8y    stop
    ${operation}=    Create Service Command Operation
    ...    ${MAPPER_XID}
    ...    {"command":"STOP","serviceName":"tedge-mapper-c8y","serviceType":"service"}
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*cannot be stopped this way.*
    ...    timeout=120
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

Restarts tedge-agent itself
    [Documentation]    tedge-agent is what runs the command, so it asks its runtime to stop and
    ...    completes the command exactly once when the workflow resumes. The action is one the
    ...    agent declares itself, so nothing has to be declared here.
    ${before}=    Get Service PID    tedge-agent

    ${operation}=    Create Service Command Operation
    ...    ${AGENT_XID}
    ...    {"command":"RESTART","serviceName":"tedge-agent","serviceType":"service"}
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

    # A service the init system manages, registered under the very name the init system knows,
    # since that is the name Cumulocity sends back and the command carries to the backend
    ThinEdgeIO.Transfer To Device    ${CURDIR}/dummy-service.service    /etc/systemd/system/
    Execute Command    systemctl daemon-reload && systemctl start ${SERVICE_NAME}

    Register Service    ${SERVICE_NAME}
    Declare Action    ${SERVICE_NAME}    start
    Declare Action    ${SERVICE_NAME}    stop
    Declare Action    ${SERVICE_NAME}    restart

Register Service
    [Arguments]    ${name}
    Execute Command    tedge mqtt pub --retain te/device/main/service/${name} '{"name":"${name}","@type":"service"}'
    Cumulocity.External Identity Should Exist    ${DEVICE_SN}:device:main:service:${name}    show_info=${False}

Stop The Mappers Connected To No Cloud
    # Leave the device as the suite setup left it. The suite teardown is called here too,
    # a test teardown of its own replacing it.
    Execute Command    systemctl stop tedge-mapper-collectd tedge-mapper-local    ignore_exit_code=${True}
    Get Logs

Declare Action
    [Arguments]    ${name}    ${action}
    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/${name}/cmd/${action} '{}'

Withdraw Action
    [Arguments]    ${name}    ${action}
    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/${name}/cmd/${action} ''

Supported Service Commands Should Be
    [Arguments]    ${external_id}    @{expected}
    Cumulocity.External Identity Should Exist    ${external_id}    show_info=${False}
    Wait Until Keyword Succeeds    30x    2s    Managed Object Service Commands Should Be    @{expected}

Managed Object Service Commands Should Be
    [Arguments]    @{expected}
    ${mo}=    Cumulocity.Managed Object Should Have Fragments    c8y_SupportedServiceCommands
    ${actual}=    Evaluate    sorted($mo["c8y_SupportedServiceCommands"])
    ${wanted}=    Evaluate    sorted($expected)
    Should Be Equal    ${actual}    ${wanted}

Create Service Command Operation
    [Arguments]    ${external_id}    ${fragment}
    Cumulocity.External Identity Should Exist    ${external_id}    show_info=${False}
    ${operation}=    Cumulocity.Create Operation
    ...    fragments={"c8y_ServiceCommand":${fragment}}
    ...    description=Service command
    RETURN    ${operation}
