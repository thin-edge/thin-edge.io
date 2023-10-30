*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             DateTime
Library             ThinEdgeIO

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

Request with non-existing log type
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"example1","searchText":"first","maximumLines":10}}
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*No such file or directory for log type: example1
    ...    timeout=120

Manual log operation request
    Execute Command    sudo -u tedge mkdir -p /var/tedge/file-transfer/${DEVICE_SN}/log_upload
    Execute Command    sudo -u tedge touch /var/tedge/file-transfer/${DEVICE_SN}/log_upload/example-1234
    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%SZ
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%SZ
    Execute Command
    ...    sudo tedge mqtt pub --retain 'te/device/main///cmd/log_upload/example-1234' '{"status":"init","tedgeUrl":"http://127.0.0.1:8000/tedge/file-transfer/${DEVICE_SN}/log_upload/example-1234","type":"example","dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","searchText":"first","lines":10}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/log_upload/example-1234
    ...    minimum=3
    ...    maximum=3


*** Keywords ***
Setup LogFiles
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/plugins/tedge-log-plugin.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/example.log    /var/log/example/
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
