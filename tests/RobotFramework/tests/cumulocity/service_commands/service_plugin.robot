*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Setup          Execute Command    rm -f ${PLUGIN_LOG}
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:services    theme:plugins


*** Variables ***
${DEVICE_SN}        ${EMPTY}
${SERVICE_NAME}     nodered
${SERVICE_TYPE}     container-test
${SERVICE_XID}      ${EMPTY}
${PLUGIN_DIR}       /usr/share/tedge/service-plugins
${PLUGIN_LOG}       /tmp/container-test-plugin.log


*** Test Cases ***
Runs the shipped restart workflow through the plugin of the service type
    ${operation}=    Create Service Command Operation
    ...    {"command":"RESTART","serviceName":"${SERVICE_NAME}","serviceType":"${SERVICE_TYPE}"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Plugin Should Have Been Called With    restart ${SERVICE_NAME}

Runs a custom freeze workflow through the plugin of the service type
    ${operation}=    Create Service Command Operation
    ...    {"command":"FREEZE","serviceName":"${SERVICE_NAME}","serviceType":"${SERVICE_TYPE}"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Plugin Should Have Been Called With    freeze ${SERVICE_NAME}

Takes the service name from the operation
    ${operation}=    Create Service Command Operation
    ...    {"command":"RESTART","serviceName":"Node-RED","serviceType":"${SERVICE_TYPE}"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ${operation_id}=    Set Variable    ${operation.to_json()["id"]}
    Should Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/cmd/restart/c8y-mapper-${operation_id}
    ...    message_contains="serviceName":"Node-RED"
    Plugin Should Have Been Called With    restart Node-RED

Takes the service name and type from the registered entity when the operation gives none
    ${operation}=    Create Service Command Operation
    ...    {"command":"RESTART"}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ${operation_id}=    Set Variable    ${operation.to_json()["id"]}
    Should Have MQTT Messages
    ...    te/device/main/service/${SERVICE_NAME}/cmd/restart/c8y-mapper-${operation_id}
    ...    message_contains="serviceType":"${SERVICE_TYPE}"
    Plugin Should Have Been Called With    restart ${SERVICE_NAME}


*** Keywords ***
Custom Setup
    ${sn}=    Setup
    Set Suite Variable    $DEVICE_SN    ${sn}
    Set Suite Variable    $SERVICE_XID    ${sn}:device:main:service:${SERVICE_NAME}
    Cumulocity.External Identity Should Exist    ${sn}    show_info=${False}

    # PLUGIN_DIR must be created by the package
    Execute Command    test -d ${PLUGIN_DIR}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/${SERVICE_TYPE}    ${PLUGIN_DIR}/
    Execute Command    chown root:root ${PLUGIN_DIR}/${SERVICE_TYPE} && chmod 755 ${PLUGIN_DIR}/${SERVICE_TYPE}

    ThinEdgeIO.Transfer To Device    ${CURDIR}/service_freeze.toml    /etc/tedge/operations/

    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/${SERVICE_NAME} '{"@type":"service","name":"${SERVICE_NAME}","type":"${SERVICE_TYPE}"}'
    Cumulocity.External Identity Should Exist    ${SERVICE_XID}    show_info=${False}

    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/${SERVICE_NAME}/cmd/restart '{}'
    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/${SERVICE_NAME}/cmd/freeze '{}'

Create Service Command Operation
    [Arguments]    ${fragment}
    Cumulocity.External Identity Should Exist    ${SERVICE_XID}    show_info=${False}
    ${operation}=    Cumulocity.Create Operation
    ...    fragments={"c8y_ServiceCommand":${fragment}}
    ...    description=Service command
    RETURN    ${operation}

Plugin Should Have Been Called With
    [Arguments]    ${arguments}
    ${calls}=    Execute Command    cat ${PLUGIN_LOG}    strip=${True}
    Should Be Equal    ${calls}    ${arguments}
