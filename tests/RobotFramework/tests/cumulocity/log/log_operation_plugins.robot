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
    Should Support Log File Types    tedge-agent::journald    includes=${True}
    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=tedge-agent::journald
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log Operation Attachment File Contains
    ...    ${operation}
    ...    expected_pattern=.*Starting tedge-agent.*

Non-existent plugin
    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=some_log::non_existent_plugin
    ${operation}=    Operation Should Be FAILED
    ...    ${operation}
    ...    timeout=120
    ...    failure_reason=.*Plugin not found.*

Dynamic plugin install
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/fake_plugin
    ...    /usr/local/lib/tedge/log-plugins/fake_plugin
    Execute Command    chmod +x /usr/local/lib/tedge/log-plugins/fake_plugin
    Should Support Log File Types    fake_log::fake_plugin    includes=${True}

    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=fake_log::fake_plugin
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log Operation Attachment File Contains
    ...    ${operation}
    ...    expected_pattern=.*Some content.*

Remove plugins dynamically
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/dummy_plugin
    ...    /usr/local/lib/tedge/log-plugins/dummy_plugin
    Execute Command    chmod +x /usr/local/lib/tedge/log-plugins/dummy_plugin
    Should Support Log File Types    dummy_log::dummy_plugin    includes=${True}

    Execute Command    rm /usr/local/lib/tedge/log-plugins/dummy_plugin

    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=dummy_log::dummy_plugin
    ${operation}=    Operation Should Be FAILED
    ...    ${operation}
    ...    timeout=120
    ...    failure_reason=.*Plugin not found.*

Agent resilient to plugin dirs removal
    ${date_from}=    Get Unix Timestamp
    Execute Command    rm -rf /usr/local/lib/tedge/log-plugins
    Should Have MQTT Messages    c8y/s/us    date_from=${date_from}    message_pattern=118,software-management

    ${date_from}=    Get Unix Timestamp
    Execute Command    rm -rf /usr/lib/tedge/log-plugins
    Should Have MQTT Messages    c8y/s/us    date_from=${date_from}    message_pattern=118,

    ${date_from}=    Get Unix Timestamp
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
    Should Have MQTT Messages    c8y/s/us    date_from=${date_from}    message_pattern=118,


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/journald
    ...    /usr/local/lib/tedge/log-plugins/journald
    Execute Command    chmod +x /usr/local/lib/tedge/log-plugins/journald

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
