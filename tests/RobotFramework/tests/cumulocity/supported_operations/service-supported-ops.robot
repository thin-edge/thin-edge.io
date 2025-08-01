*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:operation


*** Test Cases ***
Services Can Register Supported Operations
    Register Service    app1    systemd
    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/app1/cmd/restart '{}'
    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/app1/cmd/log_upload '{"types": ["systemd"]}'

    Cumulocity.Should Contain Supported Operations    c8y_Restart    c8y_LogfileRequest
    Cumulocity.Should Support Log File Types    systemd

Services Can Receive Operations
    # NOTE: The c8y_Restart operation is used to check the handling of operation status updates because it
    # it is easy and does not require any additional arguments
    Register Service    app2    container
    Execute Command    tedge mqtt pub -q 1 --retain te/device/main/service/app2/cmd/restart '{}'
    Cumulocity.Should Contain Supported Operations    c8y_Restart

    ${operation}    Cumulocity.Create Operation    {"c8y_Restart": {}}    description=Restart Service
    ${operation_id}    Set Variable    ${operation.operation.id}
    ${cmd_id}    Set Variable    c8y-mapper-${operation_id}

    Should Have MQTT Messages
    ...    te/device/main/service/app2/cmd/restart/${cmd_id}
    ...    message_pattern=.*init.*
    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/app2/cmd/restart/${cmd_id} '{"status":"executing"}'
    Execute Command
    ...    tedge mqtt pub -q 1 --retain te/device/main/service/app2/cmd/restart/${cmd_id} '{"status":"successful"}'

    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

Register Service
    [Arguments]    ${name}    ${service_type}=service
    ${external_id}    Set Variable    ${DEVICE_SN}:device:main:service:${name}
    Execute Command
    ...    tedge http post /te/v1/entities '{"@topic-id": "device/main/service/${name}","@id":"${external_id}", "@type": "service","name":"${name}", "type":"${service_type}"}'

    Cumulocity.Set Managed Object    ${DEVICE_SN}
    Cumulocity.Should Have Services    name=${name}    service_type=${service_type}    status=up
    Cumulocity.External Identity Should Exist    ${external_id}    show_info=${False}
    RETURN    ${external_id}
