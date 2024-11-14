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

    ${status}=    Execute Command
    ...    curl -o /dev/null --silent --write-out "%\{http_code\}" -X DELETE http://localhost:8000/tedge/entity-store/v1/entities/device/child01//
    Should Be Equal    ${status}    200

    ${get}=    Execute Command
    ...    curl -o /dev/null --silent --write-out "%\{http_code\}" http://localhost:8000/tedge/entity-store/v1/entities/device/child01//
    Should Be Equal    ${get}    404

MQTT HTTP interoperability
    Execute Command    tedge mqtt pub --retain 'te/device/child02//' '{"@type":"child-device"}'
    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_contains=101,${DEVICE_SN}:device:child02

    ${get}=    Execute Command    curl http://localhost:8000/tedge/entity-store/v1/entities/device/child02//
    Should Be Equal
    ...    ${get}
    ...    {"@topic-id":"device/child02//","@parent":"device/main//","@type":"child-device","@id":"device:child02"}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
