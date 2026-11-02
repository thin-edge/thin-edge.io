*** Settings ***
Resource            ../../../resources/common.resource
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:services    theme:operation


*** Variables ***
${DEVICE_SN}    ${EMPTY}


*** Test Cases ***
Services Can Receive Operations
    # NOTE: The c8y_Restart operation is used to check the handling of operation status updates because it
    # is easy and does not require any additional arguments
    Register Service    app2    container
    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/app2/cmd/restart '{}'
    Cumulocity.Should Contain Supported Operations    c8y_Restart

    ${operation}=    Cumulocity.Create Operation    {"c8y_Restart": {}}    description=Restart Service
    ${operation_id}=    Set Variable    ${operation.operation.id}
    ${cmd_id}=    Set Variable    c8y-mapper-${operation_id}

    Should Have MQTT Messages
    ...    te/device/main/service/app2/cmd/restart/${cmd_id}
    ...    message_pattern=.*init.*
    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/app2/cmd/restart/${cmd_id} '{"status":"executing"}'
    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/app2/cmd/restart/${cmd_id} '{"status":"successful"}'

    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}

Runs a custom operation on a service
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_DumpState.template    /etc/tedge/operations/c8y/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/dump_state.toml    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/dump_state.sh    /etc/tedge/operations/
    Execute Command    chmod a+x /etc/tedge/operations/dump_state.sh
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Restart Service    tedge-mapper-c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/mosquitto '{"@type":"service","name":"mosquitto"}'
    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/mosquitto/cmd/dump_state '{}'
    Cumulocity.External Identity Should Exist    ${DEVICE_SN}:device:main:service:mosquitto    show_info=${False}
    Cumulocity.Should Contain Supported Operations    c8y_DumpState
    ThinEdgeIO.Symlink Should Exist
    ...    /etc/tedge/operations/c8y/${DEVICE_SN}:device:main:service:mosquitto/c8y_DumpState

    ${operation}=    Cumulocity.Create Operation
    ...    fragments={"c8y_DumpState":{}}
    ...    description=Dump the state of mosquitto
    Should Have MQTT Messages
    ...    c8y/s/us/${DEVICE_SN}:device:main:service:mosquitto
    ...    message_pattern=^506,[0-9]+,active$
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}

Uploads the journal of tedge-agent from its service
    ThinEdgeIO.Transfer To Device    ${CURDIR}/service_log_upload.toml    /etc/tedge/operations/
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/tedge-agent/cmd/log_upload '{"types":["tedge-agent"]}'
    Cumulocity.External Identity Should Exist    ${DEVICE_SN}:device:main:service:tedge-agent    show_info=${False}
    Cumulocity.Should Contain Supported Operations    c8y_LogfileRequest    c8y_ServiceCommand
    Cumulocity.Should Have Exact Supported Log Types    tedge-agent
    Cumulocity.Managed Object Should Have Fragment Values    c8y_SupportedServiceCommands\=["restart"]

    ${operation}=    Cumulocity.Get Log File    type=tedge-agent
    ${operation}=    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    Cumulocity.Event Should Have An Attachment
    ...    ${event_url_parts}[-2]
    ...    expected_pattern=.*tedge-agent\\[[0-9]+\\]:.*
    ...    encoding=utf-8


*** Keywords ***
Custom Setup
    ${sn}=    Setup
    Set Suite Variable    $DEVICE_SN    ${sn}
    Cumulocity.External Identity Should Exist    ${sn}    show_info=${False}

Register Service
    [Arguments]    ${name}    ${service_type}=service
    ${external_id}=    Set Variable    ${DEVICE_SN}:device:main:service:${name}
    Execute Command
    ...    tedge http post /te/v1/entities '{"@topic-id": "device/main/service/${name}","@id":"${external_id}", "@type": "service","name":"${name}", "type":"${service_type}"}'

    Cumulocity.Set Managed Object    ${DEVICE_SN}
    Cumulocity.Should Have Services    name=${name}    service_type=${service_type}    status=up
    Cumulocity.External Identity Should Exist    ${external_id}    show_info=${False}
