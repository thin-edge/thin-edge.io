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
    ...    curl -X POST http://localhost:8000/tedge/entity-store/v1/entities -H 'Content-Type: application/json' -d '{"@topic-id": "device/child01//", "@type": "child-device"}'

    ${get}=    Execute Command    curl http://localhost:8000/tedge/entity-store/v1/entities/device/child01//
    Should Be Equal
    ...    ${get}
    ...    {"@topic-id":"device/child01//","@parent":"device/main//","@type":"child-device"}
    Should Have MQTT Messages
    ...    te/device/child01//
    ...    message_contains="@type":"child-device"

    ${entities}=    Execute Command    curl http://localhost:8000/tedge/entity-store/v1/entities
    Should Contain    ${entities}    {"@topic-id":"device/child01//","@parent":"device/main//","@type":"child-device"}

    ${status}=    Execute Command
    ...    curl -o /dev/null --silent --write-out "%\{http_code\}" -X DELETE http://localhost:8000/tedge/entity-store/v1/entities/device/child01//
    Should Be Equal    ${status}    200

    ${get}=    Execute Command
    ...    curl -o /dev/null --silent --write-out "%\{http_code\}" http://localhost:8000/tedge/entity-store/v1/entities/device/child01//
    Should Be Equal    ${get}    404

Query api
    # Build the entity tree:
    # main
    # |--- child0
    # |    |--- child00
    # |    |    |--- child000
    # |--- child1
    # |--- child2
    # |    |--- child20
    # |    |--- child21
    # |    |    |--- child210
    # |    |    |    |-- child2100
    # |    |--- child22
    Register Entity    device/child0//    child-device    device/main//
    Register Entity    device/child00//    child-device    device/child0//
    Register Entity    device/child000//    child-device    device/child00//
    Register Entity    device/child1//    child-device    device/main//
    Register Entity    device/child2//    child-device    device/main//
    Register Entity    device/child20//    child-device    device/child2//
    Register Entity    device/child21//    child-device    device/child2//
    Register Entity    device/child210//    child-device    device/child21//
    Register Entity    device/child2100//    child-device    device/child210//
    Register Entity    device/child22//    child-device    device/child2//

    ${entities}=    List Entities

    Should Contain Entity    {"@topic-id": "device/main//", "@type": "device"}    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child0//","@parent":"device/main//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child00//","@parent":"device/child0//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child000//","@parent":"device/child00//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child1//","@parent":"device/main//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2//","@parent":"device/main//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child20//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child21//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child210//","@parent":"device/child21//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2100//","@parent":"device/child210//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child22//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}

MQTT HTTP interoperability
    Execute Command    tedge mqtt pub --retain 'te/device/child_abc//' '{"@type":"child-device"}'
    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_contains=101,${DEVICE_SN}:device:child_abc

    ${get}=    Execute Command    curl http://localhost:8000/tedge/entity-store/v1/entities/device/child_abc//
    Should Be Equal
    ...    ${get}
    ...    {"@topic-id":"device/child_abc//","@parent":"device/main//","@type":"child-device"}

Entity auto-registration over MQTT
    Execute Command    tedge mqtt pub te/device/auto_child/service/collectd/m/ram '{"current": 6 }'
    Should Have MQTT Messages
    ...    te/device/auto_child//
    ...    message_contains={"@parent":"device/main//","@type":"child-device","name":"auto_child"}
    Should Have MQTT Messages
    ...    te/device/auto_child/service/collectd
    ...    message_contains={"@parent":"device/auto_child//","@type":"service","name":"collectd","type":"service"}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
