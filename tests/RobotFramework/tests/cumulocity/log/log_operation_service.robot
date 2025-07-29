*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log


*** Test Cases ***
Service can receive operation
    Execute Command    tedge mqtt pub --retain 'te/device/main/service/nginx' '{"@type":"service"}'
    Execute Command    tedge mqtt pub --retain te/device/main/service/nginx/cmd/restart '{}'
    Execute Command    tedge mqtt pub --retain te/device/main/service/nginx/cmd/log_upload '{"types": ["container"]}'

    ${service_xid}    Set Variable    ${DEVICE_SN}:device:main:service:nginx
    External Identity Should Exist    ${service_xid}    show_info=False
    Should Contain Supported Operations    c8y_Restart    c8y_LogfileRequest
    Should Support Log File Types    container    includes=${True}

    ${operation}    Create Operation    {"c8y_Restart": {}}
    ${operation_id}    Set Variable    ${operation.operation.id}

    ${cmd_id}    Set Variable    c8y-mapper-${operation_id}

    Should Have MQTT Messages
    ...    te/device/main/service/nginx/cmd/restart/${cmd_id}
    ...    message_pattern=.*init.*
    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/nginx/cmd/restart/${cmd_id} '{"status":"executing"}'
    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/nginx/cmd/restart/${cmd_id} '{"status":"successful"}'

    Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
