*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:c8y    theme:flows


*** Test Cases ***
Extend C8Y mapper with user-provided flows
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/collectd.js    /etc/tedge/mappers/c8y/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/collectd.toml    /etc/tedge/mappers/c8y/flows/
    Wait For The Flow To Reload    ${start}    collectd.toml

    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub collectd/a/b/c 12345:6789
    ${measurements}    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=collectd
    ...    value=b
    ...    series=c
    Should Be Equal As Numbers    ${measurements[0]["b"]["c"]["value"]}    6789.0


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y

Custom Teardown
    Get Suite Logs

Wait For The Flow To Reload
    [Arguments]    ${start}    ${flow}
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-c8y/status/flows
    ...    date_from=${start}
    ...    message_contains=${flow}
