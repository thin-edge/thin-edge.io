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
    ...    curl -X POST http://localhost:8000/tedge/v1/entities -H 'Content-Type: application/json' -d '{"@topic-id": "device/child01//", "@type": "child-device"}'
    Should Have MQTT Messages
    ...    te/device/child01//
    ...    message_contains="@type":"child-device"

    ${get}=    Execute Command    curl http://localhost:8000/tedge/v1/entities/device/child01//
    Should Be Equal
    ...    ${get}
    ...    {"@topic-id":"device/child01//","@parent":"device/main//","@type":"child-device"}

    ${entities}=    Execute Command    curl http://localhost:8000/tedge/v1/entities
    Should Contain
    ...    ${entities}
    ...    {"@topic-id":"device/child01//","@parent":"device/main//","@type":"child-device"}

    ${timestamp}=    Get Unix Timestamp
    ${delete}=    Execute Command
    ...    curl --silent -X DELETE http://localhost:8000/tedge/v1/entities/device/child01//
    Should Be Equal
    ...    ${delete}
    ...    [{"@topic-id":"device/child01//","@parent":"device/main//","@type":"child-device"}]
    Should Have MQTT Messages
    ...    te/device/child01//
    ...    date_from=${timestamp}

    ${get}=    Execute Command
    ...    curl -o /dev/null --silent --write-out "%\{http_code\}" http://localhost:8000/tedge/v1/entities/device/child01//
    Should Be Equal    ${get}    404

MQTT HTTP interoperability
    Execute Command    tedge mqtt pub --retain 'te/device/child_abc//' '{"@type":"child-device"}'
    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_contains=101,${DEVICE_SN}:device:child_abc

    ${get}=    Execute Command    curl http://localhost:8000/tedge/v1/entities/device/child_abc//
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
    ...    message_contains={"@parent":"device/auto_child//","@type":"service","name":"collectd"}

Delete entity tree
    Register Entity    device/child0//    child-device    device/main//
    Register Entity    device/child1//    child-device    device/main//
    Register Entity    device/child2//    child-device    device/main//
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
    ...    {"@topic-id":"device/child0/service/service0","@parent":"device/child0//","@type":"service"}
    ...    ${deleted}
    Should Contain Entity
    ...    {"@topic-id":"device/child000//","@parent":"device/child00//","@type":"child-device"}
    ...    ${deleted}

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

    # Assert the remaining entities
    Should Contain Entity
    ...    {"@topic-id":"device/child1//","@parent":"device/main//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2//","@parent":"device/main//","@type":"child-device"}
    ...    ${entities}

Entity twin fragment apis
    ${put}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT http://localhost:8000/tedge/v1/entities/device/main///twin/maintenance_window -H 'Content-Type: application/json' -d '5'
    Should Be Equal    ${put}    5|200
    Should Have MQTT Messages
    ...    te/device/main///twin/maintenance_window
    ...    message_contains=5

    # Assert PUT is idempotent
    ${put}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT http://localhost:8000/tedge/v1/entities/device/main///twin/maintenance_window -H 'Content-Type: application/json' -d '5'
    Should Be Equal    ${put}    5|200

    ${get}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" http://localhost:8000/tedge/v1/entities/device/main///twin/maintenance_window
    Should Be Equal    ${get}    5|200

    ${timestamp}=    Get Unix Timestamp
    ${http_code}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -X DELETE http://localhost:8000/tedge/v1/entities/device/main///twin/maintenance_window
    Should Be Equal    ${http_code}    204
    Should Have MQTT Messages
    ...    te/device/main///twin/maintenance_window
    ...    date_from=${timestamp}
    ${retained_message}=    Execute Command
    ...    tedge mqtt sub --no-topic te/device/main///twin/maintenance_window --duration 1s
    ...    ignore_exit_code=${True}
    ...    strip=${True}
    Should Be Empty    ${retained_message}

    # Assert DELETE is idempotent
    ${http_code}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -X DELETE http://localhost:8000/tedge/v1/entities/device/main///twin/maintenance_window
    Should Be Equal    ${http_code}    204

Entity twin apis
    # Assert PUT response
    ${payload}=    Set Variable    {"maintainer":"John Doe","maintenance_mode":true,"maintenance_window":5}
    ${put}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT http://localhost:8000/tedge/v1/entities/device/main///twin -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal    ${put}    ${payload}|200
    Should Have MQTT Messages
    ...    te/device/main///twin/maintenance_mode
    ...    message_contains=true
    Should Have MQTT Messages
    ...    te/device/main///twin/maintenance_window
    ...    message_contains=5

    # Assert GET response
    ${get}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" http://localhost:8000/tedge/v1/entities/device/main///twin
    Should Be Equal
    ...    ${get}
    ...    ${payload}|200

    # Replace existing twins
    ${timestamp}=    Get Unix Timestamp
    ${new_payload}=    Set Variable
    ...    {"last_serviced":"2025-01-01","maintainer":"John Doe","maintenance_mode":false}
    ${put}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT http://localhost:8000/tedge/v1/entities/device/main///twin -H 'Content-Type: application/json' -d '${new_payload}'
    Should Be Equal    ${put}    ${new_payload}|200
    Should Have MQTT Messages
    ...    te/device/main///twin/maintenance_mode
    ...    message_contains=false
    ...    date_from=${timestamp}
    Should Have MQTT Messages
    ...    te/device/main///twin/last_serviced
    ...    message_contains="2025-01-01"
    ...    date_from=${timestamp}
    # Unchanged values are not re-published
    Should Have MQTT Messages
    ...    te/device/main///twin/maintainer
    ...    date_from=${timestamp}
    ...    minimum=0
    # Removed values are cleared
    Should Not Have Retained MQTT Messages    topic=te/device/main///twin/maintenance_window

    # Assert PUT is idempotent
    ${put}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT http://localhost:8000/tedge/v1/entities/device/main///twin -H 'Content-Type: application/json' -d '${new_payload}'
    Should Be Equal    ${put}    ${new_payload}|200

    # Assert GET response
    ${get}=    Execute Command
    ...    curl http://localhost:8000/tedge/v1/entities/device/main///twin
    Should Be Equal
    ...    ${get}
    ...    ${new_payload}

    # Assert DELETE response
    ${timestamp}=    Get Unix Timestamp
    ${http_code}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -X DELETE http://localhost:8000/tedge/v1/entities/device/main///twin
    Should Be Equal    ${http_code}    204
    Should Have MQTT Messages
    ...    te/device/main///twin/maintenance_mode
    ...    date_from=${timestamp}
    ${retained_message}=    Execute Command
    ...    tedge mqtt sub --no-topic te/device/main///twin/maintenance_mode --duration 1s
    ...    ignore_exit_code=${True}
    ...    strip=${True}
    Should Be Empty    ${retained_message}

    # Assert DELETE is idempotent
    ${http_code}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -X DELETE http://localhost:8000/tedge/v1/entities/device/main///twin
    Should Be Equal    ${http_code}    204

    ${put}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT http://localhost:8000/tedge/v1/entities/device/main///twin -H 'Content-Type: application/json' -d {}
    Should Be Equal    ${put}    {}|200

Entity twin api errors
    # Get twin data of non-existent entity
    ${url}=    Set Variable    http://localhost:8000/tedge/v1/entities/device/bad-child///twin
    ${resp}=    Execute Command    curl --silent --write-out "|%\{http_code\}" ${url}
    Should Be Equal
    ...    ${resp}
    ...    {"error":"The specified entity: device/bad-child// does not exist in the store"}|404

    # Set twin fragments with non JSON map payload
    ${url}=    Set Variable
    ...    http://localhost:8000/tedge/v1/entities/device/main///twin
    ${payload}=    Set Variable    true
    ${resp}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal
    ...    ${resp}
    ...    {"error":"invalid type: boolean `true`, expected a map at line 1 column 4"}|400

    # Unsupported PATCH method on twin path
    ${url}=    Set Variable    http://localhost:8000/tedge/v1/entities/device/main///twin
    ${payload}=    Set Variable    {"maintenance_mode":true}
    ${resp}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -X PATCH ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal    ${resp}    405

    # Set twin fragment with bad key
    ${url}=    Set Variable
    ...    http://localhost:8000/tedge/v1/entities/device/main///twin/multi/path/key
    ${payload}=    Set Variable    true
    ${resp}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal
    ...    ${resp}
    ...    {"error":"Invalid twin key: 'multi/path/key'. Keys that are empty, containing '/' or starting with '@' are not allowed"}|400

    # Set twin fragment with bad value
    ${url}=    Set Variable
    ...    http://localhost:8000/tedge/v1/entities/device/main///twin/test_key
    ${payload}=    Set Variable    1.2.3
    ${resp}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal
    ...    ${resp}
    ...    {"error":"trailing characters at line 1 column 4"}|400

    # Set twin fragment with bad value
    ${url}=    Set Variable
    ...    http://localhost:8000/tedge/v1/entities/device/main///twin/test_key
    ${payload}=    Set Variable    1-2
    ${resp}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal
    ...    ${resp}
    ...    {"error":"trailing characters at line 1 column 2"}|400

    # Set twin fragment with bad value
    ${url}=    Set Variable
    ...    http://localhost:8000/tedge/v1/entities/device/main///twin/test_key
    ${payload}=    Set Variable    true true
    ${resp}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal
    ...    ${resp}
    ...    {"error":"trailing characters at line 1 column 6"}|400

    # Set twin fragment with bad value
    ${url}=    Set Variable
    ...    http://localhost:8000/tedge/v1/entities/device/main///twin/test_key
    ${payload}=    Set Variable    {"a":1}{"b":2}
    ${resp}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal
    ...    ${resp}
    ...    {"error":"trailing characters at line 1 column 8"}|400

    # Unsupported PATCH method on twin fragment path
    ${url}=    Set Variable    http://localhost:8000/tedge/v1/entities/device/main///twin/maintenance_mode
    ${payload}=    Set Variable    true
    ${resp}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -X PATCH ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal    ${resp}    405

    # Unsupported PATCH method on twin path
    ${url}=    Set Variable    http://localhost:8000/tedge/v1/entities/device/main///twin
    ${payload}=    Set Variable    true
    ${resp}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -X PATCH ${url} -H 'Content-Type: application/json' -d '${payload}'
    Should Be Equal    ${resp}    405

    # Unsupported channel
    ${url}=    Set Variable    http://localhost:8000/tedge/v1/entities/device/bad-child///cmd/123
    ${resp}=    Execute Command    curl --silent --write-out "|%\{http_code\}" ${url}
    Should Be Equal    ${resp}    {"error":"Actions on channel: cmd are not supported"}|404

    # Payload exceeds 1MB size limit
    ${url}=    Set Variable    http://localhost:8000/tedge/v1/entities/device/main///twin/key
    Execute Command    echo -n '"' > payload.txt
    Execute Command    yes x | head -n 1048576 | tr -d '\n' >> payload.txt
    Execute Command    echo -n '"' >> payload.txt
    ${resp}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" -X PUT ${url} -H 'Content-Type: application/json' --data-binary @payload.txt
    Should Be Equal
    ...    ${resp}
    ...    Failed to buffer the request body: length limit exceeded|413

Delete entity clears entity registration and twin messages
    Register Entity    device/child0//    child-device    device/main//
    Register Entity    device/child1//    child-device    device/main//
    Register Entity    device/child2//    child-device    device/main//
    Register Entity    device/child0/service/service0    service    device/child0//
    Register Entity    device/child00//    child-device    device/child0//
    Register Entity    device/child000//    child-device    device/child00//

    Execute Command    tedge http put /tedge/v1/entities/device/child0///twin '{"x": 1, "y": 2, "z": 3}'
    Execute Command    tedge mqtt pub --retain 'te/device/child0///twin/foo' '"bar"'
    Execute Command    tedge mqtt pub --retain 'te/device/child0/service/service0/twin/foo' '"bar"'
    Execute Command    tedge mqtt pub --retain 'te/device/child00///twin/foo' '"bar"'
    Execute Command    tedge mqtt pub --retain 'te/device/child000///twin/foo' '"bar"'

    Should Have Retained MQTT Messages    te/device/child0///twin/foo    message_contains="bar"
    Should Have Retained MQTT Messages    te/device/child0///twin/x    message_contains=1
    Should Have Retained MQTT Messages    te/device/child0///twin/y    message_contains=2
    Should Have Retained MQTT Messages    te/device/child0///twin/z    message_contains=3
    Should Have Retained MQTT Messages    te/device/child00///twin/foo    message_contains="bar"
    Should Have Retained MQTT Messages    te/device/child000///twin/foo    message_contains="bar"

    ${deleted}=    Deregister Entity    device/child0//

    Should Not Have Retained MQTT Messages    te/device/child0//#
    Should Not Have Retained MQTT Messages    te/device/child00//#
    Should Not Have Retained MQTT Messages    te/device/child000//#


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
