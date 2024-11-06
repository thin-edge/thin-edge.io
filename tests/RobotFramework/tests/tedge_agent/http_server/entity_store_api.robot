*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs    ${DEVICE_SN}

Test Tags           theme:tedge_agent


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Main device serial number


*** Test Cases ***
CRUD apis
    Execute Command
    ...    curl -X POST http://localhost:8000/tedge/entity-store/v1/entities/device/child01// -H 'Content-Type: application/json' -d '{"@topic-id": "device/child01//", "@type": "child-device"}'

    ${get}=    Execute Command    curl http://localhost:8000/tedge/entity-store/v1/entities/device/child01//
    Should Be Equal
    ...    ${get}
    ...    {"@topic-id":"device/child01//","@parent":"device/main//","@type":"child-device","@id":"device:child01"}
    Should Have MQTT Messages
    ...    te/device/child01//
    ...    message_contains="@type":"child-device"


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
