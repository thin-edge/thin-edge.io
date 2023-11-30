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
Successful log operation
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
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
    ...    failure_reason=.*No logs found for log type example1
    ...    timeout=120

Manual log_upload operation request
    Execute Command    sudo -u tedge mkdir -p /var/tedge/file-transfer/${DEVICE_SN}/log_upload
    Execute Command    sudo -u tedge touch /var/tedge/file-transfer/${DEVICE_SN}/log_upload/example-1234
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%SZ
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%SZ
    Publish and Verify Local Command    
    ...    topic=te/device/main///cmd/log_upload/example-1234
    ...    payload={"status":"init","tedgeUrl":"http://127.0.0.1:8000/tedge/file-transfer/${DEVICE_SN}/log_upload/example-1234","type":"example","dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","searchText":"first","lines":10}
    ...    c8y_fragment=c8y_DownloadConfigFile

Log operation successful when file transfer service on different host
    ${parent_ip}=    Get IP Address

    ${CHILD_SN}=    Setup    skip_bootstrap=True
    Set Device Context    ${CHILD_SN}
    
    # Set up a child device with only tedge-agent and connect both devices' MQTT broker and HTTP file transfer server
    Execute Command    dpkg -i packages/tedge_*.deb packages/tedge-agent_*.deb
    Execute Command    tedge config set http.bind.address 0.0.0.0
    Execute Command    tedge config set mqtt.client.host ${parent_ip}
    Restart Service    tedge-agent
    ${child_ip}=       Get IP Address

    Set Device Context    ${DEVICE_SN}
    Execute Command    tedge config set mqtt.bind.address 0.0.0.0
    Execute Command    tedge config set http.client.host ${child_ip}
    Execute Command    tedge reconnect c8y
    Stop Service       tedge-agent
    Start Service      tedge-log-plugin


    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=     Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"example","searchText":"first","maximumLines":10}}
    Operation Should Be SUCCESSFUL    ${operation}


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
    Log File Contents Should Be Equal    ${operation}    filename: logfile.3.log\n${logfile3_contents}\nfilename: logfile.2.log\n${logfile2_contents}\n

*** Keywords ***
Setup LogFiles
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/plugins/tedge-log-plugin.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/example.log    /var/log/example/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/logfile.*.log    /var/log/example/
    # touch file again to change last modified timestamp, otherwise the logfile retrieval could be outside of the requested range
    Execute Command
    ...    chown root:root /etc/tedge/plugins/tedge-log-plugin.toml /var/log/example/example.log && touch /var/log/example/example.log
    ThinEdgeIO.Service Health Status Should Be Up    tedge-log-plugin
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Setup LogFiles

Publish and Verify Local Command
    [Arguments]    ${topic}    ${payload}    ${expected_status}=successful    ${c8y_fragment}=
    [Teardown]    Execute Command    tedge mqtt pub --retain '${topic}' ''
    Execute Command    tedge mqtt pub --retain '${topic}' '${payload}'
    ${messages}=    Should Have MQTT Messages    ${topic}    minimum=1    maximum=1    message_contains="status":"${expected_status}"

    Sleep    5s    reason=Given mapper a chance to react, if it does not react with 5 seconds it never will
    ${retained_message}    Execute Command    timeout 1 tedge mqtt sub --no-topic '${topic}'    ignore_exit_code=${True}    strip=${True}
    Should Be Equal    ${messages[0]}    ${retained_message}    msg=MQTT message should be unchanged

    IF    "${c8y_fragment}"
        # There should not be any c8y related operation transition messages sent: https://cumulocity.com/guides/reference/smartrest-two/#updating-operations
        Should Have MQTT Messages    c8y/s/ds    message_pattern=^(501|502|503),${c8y_fragment}.*    minimum=0    maximum=0
    END

Log File Contents Should Be Equal
    [Arguments]    ${operation}    ${expected_contents}    ${encoding}=utf-8
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    ${event_id}=    Set Variable    ${event_url_parts}[-2]
    ${contents}=    Cumulocity.Event Should Have An Attachment    ${event_id}    expected_contents=${expected_contents}    encoding=${encoding}
    RETURN    ${contents}
