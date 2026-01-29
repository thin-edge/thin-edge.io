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
    ${message}    Should Have MQTT Messages
    ...    topic=c8y/measurement/measurements/create
    ...    message_contains="type":"collectd"

    Cumulocity.Set Managed Object    ${DEVICE_SN}
    ${measurements}    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=collectd
    ...    value=b
    ...    series=c
    Should Be Equal As Numbers    ${measurements[0]["b"]["c"]["value"]}    6789.0

Get entity metadata from the c8y mapper context
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-measurements.js    /etc/tedge/mappers/c8y/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-measurements.toml    /etc/tedge/mappers/c8y/flows/
    Wait For The Flow To Reload    ${start}    custom-measurements.toml

    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub custom/device/child///m/temperature 23.1
    ${message}    Should Have MQTT Messages
    ...    topic=c8y/measurement/measurements/create
    ...    message_contains=temperature
    ...    date_from=${start}
    Should Contain    ${message}[0]    "type":"custom"
    Should Contain    ${message}[0]    "externalId":"${CHILD_SN}"

    Cumulocity.Set Managed Object    ${CHILD_SN}
    ${measurements}    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    value=temperature
    ...    series=temperature
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    23.1


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}-child
    Execute Command    tedge mqtt pub --retain 'te/device/child//' '{"@type":"child-device","@id":"${CHILD_SN}"}'
    Device Should Exist    ${CHILD_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y

Custom Teardown
    Get Suite Logs

Wait For The Flow To Reload
    [Arguments]    ${start}    ${flow}
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-c8y/status/flows
    ...    date_from=${start}
    ...    message_contains=${flow}
