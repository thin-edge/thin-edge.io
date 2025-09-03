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
Log operation journald plugin
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"tedge-agent.service::journald","searchText":"","maximumLines":100}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log Operation Attachment File Contains
    ...    ${operation}
    ...    expected_pattern=.*COMMAND=/etc/tedge/log-plugins/journald get tedge-agent.service.*


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/*
    ...    /etc/tedge/log-plugins/
    Execute Command    chmod +x /etc/tedge/log-plugins/journald
    Execute Command
    ...    cmd=echo "tedge ALL = (ALL) NOPASSWD:SETENV: /etc/tedge/log-plugins/[a-zA-Z0-9]**" >> /etc/sudoers.d/tedge

    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Create Log Request Operation
    [Arguments]    ${start_timestamp}
    ...    ${end_timestamp}
    ...    ${log_type}
    ...    ${search_text}=${EMPTY}
    ...    ${maximum_lines}=1000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"${log_type}","searchText":"${search_text}","maximumLines":${maximum_lines}}}
    RETURN    ${operation}

Log Operation Attachment File Contains
    [Arguments]    ${operation}    ${expected_pattern}
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    ${event_id}=    Set Variable    ${event_url_parts}[-2]
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${event_id}
    ...    expected_pattern=${expected_pattern}
    ...    encoding=utf-8
