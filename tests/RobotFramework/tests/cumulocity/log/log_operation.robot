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
    ${end_timestamp}=    get current date    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"1970-01-01T00:00:00+0000","dateTo":"${end_timestamp}","logFile":"example","searchText":"first","maximumLines":10}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

Request with non-existing log type
    ${end_timestamp}=    get current date    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"1970-01-01T00:00:00+0000","dateTo":"${end_timestamp}","logFile":"example1","searchText":"first","maximumLines":10}}
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*No such file or directory for log type: example1
    ...    timeout=120


*** Keywords ***
Setup LogFiles
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/plugins/tedge-log-plugin.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/example.log    /var/log/example/
    Execute Command    chown root:root /etc/tedge/plugins/tedge-log-plugin.toml /var/log/example/example.log
    ThinEdgeIO.Restart Service    tedge-log-plugin
    ThinEdgeIO.Service Health Status Should Be Up    tedge-log-plugin
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Execute Command    sudo tedge config set c8y.enable.log_management true
    ThinEdgeIO.Restart Service    tedge-mapper-c8y

    Setup LogFiles
