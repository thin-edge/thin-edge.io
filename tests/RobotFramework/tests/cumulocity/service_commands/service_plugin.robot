*** Settings ***
Documentation       Service commands of a service that the init system does not manage. The
...                 service type selects a service plugin, which runs the standard actions and
...                 any custom action of its own.

Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:services    theme:plugins


*** Variables ***
${DEVICE_SN}        ${EMPTY}
${SERVICE_NAME}     nodered
${SERVICE_TYPE}     container
${SERVICE_XID}      ${EMPTY}
${PLUGIN_DIR}       /usr/share/tedge/service-plugins
${PLUGIN_LOG}       /tmp/container-plugin.log


*** Test Cases ***
Ships a plugin directory the tedge user cannot write to
    ${owner}=    Execute Command    stat -c '%U %a' ${PLUGIN_DIR}    strip=${True}
    Should Be Equal    ${owner}    root 755

Declares the actions of the service to Cumulocity
    Cumulocity.External Identity Should Exist    ${SERVICE_XID}    show_info=${False}
    Cumulocity.Should Contain Supported Operations    c8y_ServiceCommand
    Supported Service Commands Should Be    PAUSE    RESTART    STOP

Runs a standard action through the plugin of the service type
    Clear Plugin Log
    ${operation}=    Run Service Command
    ...    {"command":"RESTART","serviceName":"${SERVICE_NAME}","serviceType":"${SERVICE_TYPE}"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Plugin Should Have Been Called With    restart ${SERVICE_NAME}

Runs a custom action through the plugin
    Clear Plugin Log
    ${operation}=    Run Service Command
    ...    {"command":"PAUSE","serviceName":"${SERVICE_NAME}","serviceType":"${SERVICE_TYPE}"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Plugin Should Have Been Called With    pause ${SERVICE_NAME}

Takes the service name from the operation
    [Documentation]    The name reaches the plugin as Cumulocity sends it. The target is resolved
    ...    from the external id, so the command is still published on the topic of that service.
    Clear Plugin Log
    ${operation}=    Run Service Command
    ...    {"command":"RESTART","serviceName":"Node-RED","serviceType":"${SERVICE_TYPE}"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ${operation_id}=    Set Variable    ${operation.to_json()["id"]}
    Should Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/cmd/restart/c8y-mapper-${operation_id}
    ...    message_contains="serviceName":"Node-RED"
    Plugin Should Have Been Called With    restart Node-RED

Takes the service type from the registered entity
    [Documentation]    The type Cumulocity sends is only a fallback: the type the service
    ...    registered itself with selects the backend.
    Clear Plugin Log
    ${operation}=    Run Service Command
    ...    {"command":"RESTART","serviceName":"${SERVICE_NAME}","serviceType":"service"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ${operation_id}=    Set Variable    ${operation.to_json()["id"]}
    Should Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/cmd/restart/c8y-mapper-${operation_id}
    ...    message_contains="serviceType":"${SERVICE_TYPE}"
    Plugin Should Have Been Called With    restart ${SERVICE_NAME}

Reports an action the plugin does not support
    Clear Plugin Log
    ${operation}=    Run Service Command
    ...    {"command":"STOP","serviceName":"${SERVICE_NAME}","serviceType":"${SERVICE_TYPE}"}
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*not supported for that type of service.*
    ...    timeout=120
    Plugin Should Have Been Called With    stop ${SERVICE_NAME}

Runs an action from the command line
    Clear Plugin Log
    Execute Command    sudo tedge service pause ${SERVICE_NAME} --service-type ${SERVICE_TYPE}
    Plugin Should Have Been Called With    pause ${SERVICE_NAME}

Exits 2 when the service type has no plugin
    Execute Command
    ...    sudo tedge service restart ${SERVICE_NAME} --service-type no_such_backend
    ...    exp_exit_code=2


*** Keywords ***
Custom Setup
    ${sn}=    Setup
    Set Suite Variable    $DEVICE_SN    ${sn}
    Set Suite Variable    $SERVICE_XID    ${sn}:device:main:service:${SERVICE_NAME}
    Cumulocity.External Identity Should Exist    ${sn}    show_info=${False}

    # Created by the packaging, so the transfer below must not be what creates it
    Execute Command    test -d ${PLUGIN_DIR}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/container    ${PLUGIN_DIR}/
    Execute Command    chown root:root ${PLUGIN_DIR}/container && chmod 755 ${PLUGIN_DIR}/container

    # `pause` is a custom action, so it has no shipped workflow
    ThinEdgeIO.Transfer To Device    ${CURDIR}/service_pause.toml    /etc/tedge/operations/
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

    Execute Command
    ...    tedge http post /te/v1/entities '{"@topic-id":"device/main/service/${SERVICE_NAME}","@parent":"device/main//","@type":"service","name":"${SERVICE_NAME}","type":"${SERVICE_TYPE}"}'
    Cumulocity.External Identity Should Exist    ${SERVICE_XID}    show_info=${False}

    Declare Action    restart
    Declare Action    stop
    Declare Action    pause

Declare Action
    [Arguments]    ${action}
    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/${SERVICE_NAME}/cmd/${action} '{}'

Supported Service Commands Should Be
    [Arguments]    @{expected}
    Cumulocity.External Identity Should Exist    ${SERVICE_XID}    show_info=${False}
    Wait Until Keyword Succeeds    30x    2s    Managed Object Service Commands Should Be    @{expected}

Managed Object Service Commands Should Be
    [Arguments]    @{expected}
    ${mo}=    Cumulocity.Managed Object Should Have Fragments    c8y_SupportedServiceCommands
    ${actual}=    Evaluate    sorted($mo["c8y_SupportedServiceCommands"])
    ${wanted}=    Evaluate    sorted($expected)
    Should Be Equal    ${actual}    ${wanted}

Run Service Command
    [Arguments]    ${fragment}
    Cumulocity.External Identity Should Exist    ${SERVICE_XID}    show_info=${False}
    ${operation}=    Cumulocity.Create Operation
    ...    fragments={"c8y_ServiceCommand":${fragment}}
    ...    description=Service command
    RETURN    ${operation}

Clear Plugin Log
    Execute Command    rm -f ${PLUGIN_LOG}

Plugin Should Have Been Called With
    [Documentation]    The plugin records every invocation, so this pins the argv the runner built
    [Arguments]    ${arguments}
    ${calls}=    Execute Command    cat ${PLUGIN_LOG}    strip=${True}
    Should Be Equal    ${calls}    ${arguments}
