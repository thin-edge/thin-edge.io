*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             DateTime
Library             ThinEdgeIO
Library             String
Library             OperatingSystem

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log


*** Test Cases ***
Log operation ignore date range when log file has a static path   
    ${start_timestamp}=    Get Current Date    UTC    +10 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"example","searchText":"first","maximumLines":10}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log File Contents Should Be Equal    ${operation}    filename: example.log\n1 first line\n

Request with non-existing log type
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"example1","searchText":"first","maximumLines":10}}
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*No logs found for log type "example1"
    ...    timeout=120

Manual log_upload operation request
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%SZ
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%SZ
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/log_upload/example-1234
    ...    payload={"status":"init","tedgeUrl":"http://127.0.0.1:8000/tedge/file-transfer/${DEVICE_SN}/log_upload/example-1234","type":"example","dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","searchText":"first","lines":10}
    ...    c8y_fragment=c8y_LogfileRequest

Trigger log_upload operation from another operation
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%SZ
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%SZ
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/sub_log_upload/example-1234
    ...    payload={"status":"init","tedgeUrl":"http://127.0.0.1:8000/tedge/file-transfer/${DEVICE_SN}/sub_log_upload/example-1234","type":"example","dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","searchText":"repeated","lines":3}
    ${log_excerpt}     Execute Command    curl http://127.0.0.1:8000/tedge/file-transfer/${DEVICE_SN}/sub_log_upload/example-1234
    Should Be Equal    ${log_excerpt}     filename: example.log\n13 repeated line\n14 repeated line\n15 repeated line\n

Trigger custom log_upload operation
    [Teardown]    Restore log_upload operation
    Customize log_upload operation
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/log_upload/custom-1234
    ...    payload={"status":"init","tedgeUrl":"http://127.0.0.1:8000/tedge/file-transfer/${DEVICE_SN}/log_upload/custom-1234","type":"example","searchText":"first","lines":10}
    ...    c8y_fragment=c8y_LogfileRequest
    ${log_excerpt}     Execute Command    curl http://127.0.0.1:8000/tedge/file-transfer/${DEVICE_SN}/log_upload/custom-1234
    ${expected_log}    Get File    ${CURDIR}/example.log
    Should Be Equal    ${log_excerpt}     ${expected_log}

Log file request limits maximum number of lines with text filter
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"example","searchText":"repeated line","maximumLines":2}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log File Contents Should Be Equal    ${operation}    filename: example.log\n14 repeated line\n15 repeated line\n

Log file request limits maximum number of lines without text filter
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"example","searchText":"","maximumLines":300}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    ${expected_contents}=    OperatingSystem.Get File    ${CURDIR}/example.log
    Log File Contents Should Be Equal    ${operation}    filename: example.log\n${expected_contents}

Log file request supports date/time filters and can search across multiple log files
    Execute Command    touch -d "48 hours ago" /var/log/example/logfile.1.log
    Execute Command    touch -d "20 hours ago" /var/log/example/logfile.2.log
    Execute Command    touch -d "6 hours ago" /var/log/example/logfile.3.log

    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request (multiple_logfiles)
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"multiple_logfiles","searchText":"","maximumLines":300}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    # Log files are search from newest to oldest
    ${logfile3_contents}=    OperatingSystem.Get File    ${CURDIR}/logfile.3.log
    ${logfile2_contents}=    OperatingSystem.Get File    ${CURDIR}/logfile.2.log
    ${expected_contents}=    OperatingSystem.Get File    ${CURDIR}/example.log
    Log File Contents Should Be Equal
    ...    ${operation}
    ...    filename: logfile.3.log\n${logfile3_contents}\nfilename: logfile.2.log\n${logfile2_contents}\n

Log file request not processed if operation is disabled for tedge-agent
    [Teardown]    Enable log upload capability of tedge-agent
    Disable log upload capability of tedge-agent
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%SZ
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%SZ
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/log_upload/example-1234
    ...    payload={"status":"init","tedgeUrl":"http://127.0.0.1:8000/tedge/file-transfer/${DEVICE_SN}/log_upload/example-1234","type":"example","dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","searchText":"first","lines":10}
    ...    expected_status=init
    ...    c8y_fragment=c8y_LogfileRequest

Default plugin configuration
    Set Device Context    ${DEVICE_SN}

    # Remove the existing plugin configuration
    Execute Command    rm /etc/tedge/plugins/tedge-log-plugin.toml

    # Agent restart should recreate the default plugin configuration
    Stop Service    tedge-agent
    Service Should Be Stopped    tedge-agent
    ${timestamp}=        Get Unix Timestamp
    Start Service    tedge-agent
    Service Should Be Running    tedge-agent

    Should Have MQTT Messages    c8y/s/us    message_contains=118,    date_from=${timestamp}
    Cumulocity.Set Device    ${DEVICE_SN}
    Cumulocity.Should Support Log File Types    software-management

    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation    ${start_timestamp}    ${end_timestamp}    log_type=software-management
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log Operation Attachment File Contains    ${operation}    expected_pattern=.*software_list @ successful

*** Keywords ***
Setup LogFiles
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/plugins/tedge-log-plugin.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/example.log    /var/log/example/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/logfile.*.log    /var/log/example/
    # touch file again to change last modified timestamp, otherwise the logfile retrieval could be outside of the requested range
    Execute Command
    ...    chown root:root /etc/tedge/plugins/tedge-log-plugin.toml /var/log/example/example.log && touch /var/log/example/example.log
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

Customize Operation Workflows
    ThinEdgeIO.Transfer To Device    ${CURDIR}/sub_log_upload.toml    /etc/tedge/operations/
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Customize log_upload operation
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom_log_upload.toml    /etc/tedge/operations/custom_log_upload.toml
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Restore log_upload operation
    Execute Command    rm -f /etc/tedge/operations/custom_log_upload.toml
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Customize Operation Workflows
    Setup LogFiles

Publish and Verify Local Command
    [Arguments]    ${topic}    ${payload}    ${expected_status}=successful    ${c8y_fragment}=
    Execute Command    tedge mqtt pub --retain '${topic}' '${payload}'
    ${messages}=    Should Have MQTT Messages
    ...    ${topic}
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="status":"${expected_status}"

    Sleep    5s    reason=Given mapper a chance to react, if it does not react with 5 seconds it never will
    ${retained_message}=    Execute Command
    ...    timeout 1 tedge mqtt sub --no-topic '${topic}'
    ...    ignore_exit_code=${True}
    ...    strip=${True}
    Should Be Equal    ${messages[0]}    ${retained_message}    msg=MQTT message should be unchanged

    IF    "${c8y_fragment}"
        # There should not be any c8y related operation transition messages sent: https://cumulocity.com/guides/reference/smartrest-two/#updating-operations
        Should Have MQTT Messages
        ...    c8y/s/ds
        ...    message_pattern=^(501|502|503),${c8y_fragment}.*
        ...    minimum=0
        ...    maximum=0
    END
    [Teardown]    Execute Command    tedge mqtt pub --retain '${topic}' ''

Log File Contents Should Be Equal
    [Arguments]    ${operation}    ${expected_contents}    ${encoding}=utf-8    ${expected_filename}=^${DEVICE_SN}_[\\w\\W]+-c8y-mapper-\\d+$    ${expected_mime_type}=text/plain
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    ${event_id}=    Set Variable    ${event_url_parts}[-2]
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${event_id}
    ...    expected_contents=${expected_contents}
    ...    encoding=${encoding}
    ${event}=    Cumulocity.Event Attachment Should Have File Info    ${event_id}    name=${expected_filename}    mime_type=${expected_mime_type}
    RETURN    ${contents}

Create Log Request Operation
    [Arguments]    ${start_timestamp}    ${end_timestamp}    ${log_type}    ${search_text}=${EMPTY}    ${maximum_lines}=1000
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
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


Disable log upload capability of tedge-agent
    [Arguments]    ${device_sn}=${DEVICE_SN}
    Execute Command    tedge config set agent.enable.log_upload false
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Service Should Be Running    tedge-agent

Enable log upload capability of tedge-agent
    [Arguments]    ${device_sn}=${DEVICE_SN}
    Execute Command    tedge config set agent.enable.log_upload true
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Service Should Be Running    tedge-agent
