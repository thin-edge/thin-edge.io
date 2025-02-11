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

    ${timestamp}=    Get Unix Timestamp
    ${delete}=    Execute Command
    ...    curl --silent -X DELETE http://localhost:8000/tedge/entity-store/v1/entities/device/child01//
    Should Be Equal
    ...    ${delete}
    ...    [{"@topic-id":"device/child01//","@parent":"device/main//","@type":"child-device"}]
    Should Have MQTT Messages
    ...    te/device/child01//
    ...    date_from=${timestamp}

    ${get}=    Execute Command
    ...    curl -o /dev/null --silent --write-out "%\{http_code\}" http://localhost:8000/tedge/entity-store/v1/entities/device/child01//
    Should Be Equal    ${get}    404

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

Delete entity tree
    Register Entity    device/child0//    child-device    device/main//
    Register Entity    device/child1//    child-device    device/main//
    Register Entity    device/child0/service/service0    service    device/child0//
    Register Entity    device/child00//    child-device    device/child0//
    Register Entity    device/child000//    child-device    device/child00//

    ${deleted}=    Deregister Entity    device/child0//
    Length Should Be    ${deleted}    4

    # Assert the deleted entities
    Should Contain Entity
    ...    {"@topic-id":"device/child0//","@parent":"device/main//","@type":"child-device"}
    ...    ${deleted}
    Should Contain Entity
    ...    {"@topic-id":"device/child00//","@parent":"device/child0//","@type":"child-device"}
    ...    ${deleted}
    Should Contain Entity
    ...    {"@topic-id":"device/child0/service/service0","@parent":"device/child0//","@type":"service","type":"service"}
    ...    ${deleted}
    Should Contain Entity
    ...    {"@topic-id":"device/child000//","@parent":"device/child00//","@type":"child-device"}
    ...    ${deleted}

    # Assert the remaining entities
    ${entities}=    List Entities
    Should Not Contain Entity
    ...    "device/child0//"
    ...    ${entities}

    Should Not Contain Entity
    ...    "device/child00//"
    ...    ${entities}

    Should Not Contain Entity
    ...    "device/child000//"
    ...    ${entities}

    Should Contain Entity
    ...    {"@topic-id":"device/child1//","@parent":"device/main//","@type":"child-device"}
    ...    ${entities}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
