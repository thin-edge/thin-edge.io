*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             OperatingSystem
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log


*** Variables ***
${DEVICE_SN}    ${EMPTY}


*** Test Cases ***
Log upload operation succeeds when File Transfer Service has TLS enabled
    [Documentation]    Issue \#4187
    ${start_time}=    Get Unix Timestamp

    ${start_timestamp}=    Get Current Date    UTC    -24 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +60 seconds    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"example","searchText":"first","maximumLines":10}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    # The init message published by the mapper must carry an https:// in tedgeUrl
    Should Have MQTT Messages
    ...    te/device/main///cmd/log_upload/+
    ...    message_contains="tedgeUrl":"https://
    ...    date_from=${start_timestamp}
    ...    minimum=1

    # Validate that all temporary files created for the log upload operation are cleaned up
    Execute Command    ls /tmp/example*    exp_exit_code=2
    Execute Command    ls /var/tedge/file-transfer/${DEVICE_SN}/log_upload/example*    exp_exit_code=2

    # Issue \#4187: deleting a log file from the file transfer service should not print a warning
    ${journal_log}=    Execute Command
    ...    journalctl -u tedge-mapper-c8y --since "@${start_time}" --no-pager
    ...    ignore_exit_code=${True}
    Should Not Contain    ${journal_log}    Failed to delete log file from file transfer service


*** Keywords ***
Setup LogFiles
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/plugins/tedge-log-plugin.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/example.log    /var/log/example/
    Execute Command
    ...    chown root:root /etc/tedge/plugins/tedge-log-plugin.toml /var/log/example/example.log && touch /var/log/example/example.log

Setup TLS For File Transfer Service
    ThinEdgeIO.Transfer To Device    ${CURDIR}/generate_certificates.sh    /etc/tedge/
    Execute Command    chmod +x /etc/tedge/generate_certificates.sh
    Execute Command    /etc/tedge/generate_certificates.sh    timeout=0

    Execute Command    sudo systemctl restart tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

    ThinEdgeIO.Disconnect Then Connect Mapper    c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Setup LogFiles
    Setup TLS For File Transfer Service
