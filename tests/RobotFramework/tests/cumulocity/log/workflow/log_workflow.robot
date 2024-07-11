*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             DateTime
Library             ThinEdgeIO
Library             String

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log

*** Test Cases ***

Custom log workflow with pre-processor
    [Documentation]    Use a custom log_upload workflow to support command based logs. It uses a custom preprocessor
    ...   step to run a custom command (via the log_upload.sh script), which can be used to read logs from any source
    ...   (sqlite in this example)
    Cumulocity.Should Support Log File Types    sqlite    includes=${True}

    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000

    ${operation}=    Cumulocity.Get Log File    type=sqlite    date_from=${start_timestamp}    date_to=${end_timestamp}    maximum_lines=10
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}
    Log File Contents Should Be Equal    operation=${operation}    expected_pattern=filename: sqlite.log\\nRunning some sqlite query...\\nParameters:\\n\\s+dateFrom=.+\\n\\s+dateTo=.+\\n


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Setup LogFiles

Setup LogFiles
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/plugins/tedge-log-plugin.toml

    # Custom workflow and handler script
    ThinEdgeIO.Transfer To Device    ${CURDIR}/log_upload.toml    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/log_upload.sh    /usr/bin/log_upload.sh

    ThinEdgeio.Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y


Log File Contents Should Be Equal
    [Arguments]    ${operation}    ${expected_pattern}    ${encoding}=utf-8    ${expected_filename}=^${DEVICE_SN}_[\\w\\W]+-c8y-mapper-\\d+$    ${expected_mime_type}=text/plain
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    ${event_id}=    Set Variable    ${event_url_parts}[-2]
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${event_id}
    ...    expected_pattern=${expected_pattern}
    ...    encoding=${encoding}
    ${event}=    Cumulocity.Event Attachment Should Have File Info    ${event_id}    name=${expected_filename}    mime_type=${expected_mime_type}
    RETURN    ${contents}
