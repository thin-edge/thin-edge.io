*** Settings ***
Library             JSONLibrary
Library             ThinEdgeIO
Resource            fs_dynamic_reload.resource

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:tedge_flows


*** Test Cases ***
List c8y specific transformers with tedge flows cli
    Install Flow    custom-measurements
    ${flows}    Execute Command    tedge flows list
    Should Contain    ${flows}    item=into-c8y-measurements
    [Teardown]    Uninstall Flow    custom-measurements

Test c8y specific transformers with tedge flows cli
    Install Flow    custom-measurements
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/mappers/local/flows/custom-measurements/message.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/mappers/local/flows/custom-measurements/message.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}
    [Teardown]    Uninstall Flow    custom-measurements

Apply c8y specific transformers with tedge-mapper local
    Install Flow    custom-measurements
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main// '{"@id": "raspberry-007", "@type": "device" }'
    Execute Command    sleep 0.5
    Execute Command    tedge mqtt pub te/device/main///m/environment '{"temperature": 258.0}'
    Should Have MQTT Messages
    ...    topic=fake/c8y/measurements
    ...    message_contains="temperature":{"temperature":{"value":258.0}}
    ...    date_from=${start}
    [Teardown]    Uninstall Flow    custom-measurements


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Start Service    tedge-mapper-local

Install Flow
    [Arguments]    ${directory}
    ${start}    Get Unix Timestamp
    Execute Command    sleep 0.1
    ThinEdgeIO.Transfer To Device    ${CURDIR}/${directory}/*    /etc/tedge/mappers/local/flows/${directory}/
    Execute Command    ls -lh /etc/tedge/mappers/local/flows/${directory}
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-local/status/flows
    ...    date_from=${start}
    ...    message_contains=${directory}

Uninstall Flow
    [Arguments]    ${directory}
    ${start}    Get Unix Timestamp
    Execute Command    cmd=rm -fr /etc/tedge/mappers/local/flows/${directory}
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-local/status/flows
    ...    date_from=${start}
    ...    message_contains=${directory}
