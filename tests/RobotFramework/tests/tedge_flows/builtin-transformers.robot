*** Settings ***
Library             JSONLibrary
Library             ThinEdgeIO
Resource            fs_dynamic_reload.resource

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:flows


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

Test c8y specific transformers with tedge flows cli and initial context
    Install Flow    custom-measurements
    ${transformed_msg}    Execute Command
    ...    tedge flows test te/device/main///m/environment '{"temperature": 258}' --context '{ "device/main//": {"@id":"raspberry-007","@type":"device","@topic-id":"device/main//","@name":"raspberry-007","@type-name":"thin-edge"} }'
    Should Contain    ${transformed_msg}    item=[fake/c8y/measurements]
    Should Contain    ${transformed_msg}    item="temperature":{"temperature":{"value":258.0}}
    Should Contain    ${transformed_msg}    item="type":"environment"
    [Teardown]    Uninstall Flow    custom-measurements

Test c8y specific transformers with tedge flows cli and initial context (provided as a file)
    Install Flow    custom-measurements
    ${transformed_msg}    Execute Command
    ...    tedge flows test te/device/child-xyz///m/environment '{"temperature": 42}' --context /etc/tedge/mappers/local/flows/custom-measurements/context.json
    Should Contain    ${transformed_msg}    item=[fake/c8y/measurements]
    Should Contain
    ...    ${transformed_msg}
    ...    item={"externalSource":{"externalId":"raspberry-007-child-xyz","type":"c8y_Serial"}
    Should Contain    ${transformed_msg}    item="temperature":{"temperature":{"value":42.0}}
    Should Contain    ${transformed_msg}    item="type":"environment"
    [Teardown]    Uninstall Flow    custom-measurements

Test c8y flows with tedge flows cli and child registration
    Skip    msg=This cannot work till the registration is extracted from c8y-mapper into a flow
    # See https://github.com/thin-edge/thin-edge.io/issues/4068
    ${installed_flows}    Execute Command    tedge flows list --mapper c8y
    Should Contain    ${installed_flows}    measurements
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-measurements/c8y.samples    /etc/tedge/data/
    ${transformed_msg}    Execute Command
    ...    cat /etc/tedge/data/c8y.samples | awk '{ print $2 }' FS\='INPUT:' | tedge flows test --mapper c8y
    ...    strip=True
    ${expected_msg}    Execute Command
    ...    cat /etc/tedge/data/c8y.samples | awk '{ if ($2) print $2 }' FS\='OUTPUT: '
    ...    strip=True
    Should Be Equal
    ...    ${transformed_msg}
    ...    ${expected_msg}

Test c8y flows with tedge flows cli and a pre-built context
    ${installed_flows}    Execute Command    tedge flows list --mapper c8y
    Should Contain    ${installed_flows}    measurements
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-measurements/context.json    /etc/tedge/data/
    ${transformed_msg}    Execute Command
    ...    tedge flows test --mapper c8y --context /etc/tedge/data/context.json te/device/child-xyz///m/environment '{"temperature": 258.0, "time":"2025-06-27T13:33:53.493Z"}'
    Should Contain
    ...    ${transformed_msg}
    ...    item="externalSource":{"externalId":"raspberry-007-child-xyz","type":"c8y_Serial"}
    Should Contain    ${transformed_msg}    item="temperature":{"temperature":{"value":258.0}}
    Should Contain    ${transformed_msg}    item="type":"environment"

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
