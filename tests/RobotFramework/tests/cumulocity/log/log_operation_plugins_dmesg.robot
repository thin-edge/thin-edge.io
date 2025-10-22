*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             OperatingSystem
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log


*** Test Cases ***
Log operation dmesg plugin
    Should Contain Supported Log Types    all::dmesg
    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Get Log File
    ...    type=all::dmesg
    ...    date_from=${start_timestamp}
    ...    date_to=${end_timestamp}
    ...    maximum_lines=100
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}
    Log Operation Attachment File Should Not Be Empty
    ...    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Log Operation Attachment File Should Not Be Empty
    [Arguments]    ${operation}
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    ${event_id}=    Set Variable    ${event_url_parts}[-2]
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${event_id}
    ...    encoding=utf-8
    ...    expected_size_min=1
    RETURN    ${contents}
