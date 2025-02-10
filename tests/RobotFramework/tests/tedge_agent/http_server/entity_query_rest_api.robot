*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs    ${DEVICE_SN}

Test Tags           theme:tedge_agent


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Main device serial number


*** Test Cases ***
Query all entities
    ${entities}=    List Entities

    Should Contain Entity    {"@topic-id": "device/main//", "@type": "device"}    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/main/service/service0","@parent":"device/main//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/main/service/service1","@parent":"device/main//","@type":"service","type":"service"}
    ...    ${entities}
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
    ...    {"@topic-id":"device/child1/service/service10","@parent":"device/child1//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2//","@parent":"device/main//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2/service/service20","@parent":"device/child2//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2/service/service21","@parent":"device/child2//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child20//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child21//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child21/service/service210","@parent":"device/child21//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child210//","@parent":"device/child21//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child210/service/service2100","@parent":"device/child210//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2100//","@parent":"device/child210//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child22//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}

Query from child root
    ${entities}=    List Entities    root=device/child2//

    Length Should Be    ${entities}    10

    Should Contain Entity
    ...    {"@topic-id":"device/child2//","@parent":"device/main//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2/service/service20","@parent":"device/child2//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2/service/service21","@parent":"device/child2//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child20//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child21//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child21/service/service210","@parent":"device/child21//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child210//","@parent":"device/child21//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child210/service/service2100","@parent":"device/child210//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2100//","@parent":"device/child210//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child22//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}

Query by parent
    ${entities}=    List Entities    parent=device/child2//

    Length Should Be    ${entities}    5

    Should Contain Entity
    ...    {"@topic-id":"device/child2/service/service20","@parent":"device/child2//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2/service/service21","@parent":"device/child2//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child20//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child21//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child22//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}

Query all children
    ${entities}=    List Entities    type=child-device
    Length Should Be    ${entities}    10

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

Query all services
    ${entities}=    List Entities    type=service

    Should Contain Entity
    ...    {"@topic-id":"device/main/service/service0","@parent":"device/main//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/main/service/service1","@parent":"device/main//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child1/service/service10","@parent":"device/child1//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2/service/service20","@parent":"device/child2//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child2/service/service21","@parent":"device/child2//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child21/service/service210","@parent":"device/child21//","@type":"service","type":"service"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child210/service/service2100","@parent":"device/child210//","@type":"service","type":"service"}
    ...    ${entities}

Query with parent and type
    ${entities}=    List Entities    parent=device/child2//    type=child-device
    Length Should Be    ${entities}    3

    Should Contain Entity
    ...    {"@topic-id":"device/child20//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child21//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}
    Should Contain Entity
    ...    {"@topic-id":"device/child22//","@parent":"device/child2//","@type":"child-device"}
    ...    ${entities}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN

    # Build the entity tree:
    # main
    # |--- service0
    # |--- service1
    # |--- child0
    # |    |--- child00
    # |    |    |--- child000
    # |--- child1
    # |    |--- service10
    # |--- child2
    # |    |--- service20
    # |    |--- service21
    # |    |--- child20
    # |    |--- child21
    # |    |    |--- service210
    # |    |    |--- child210
    # |    |    |    |-- service2100
    # |    |    |    |-- child2100
    # |    |--- child22
    Register Entity    device/main/service/service0    service    device/main//
    Register Entity    device/main/service/service1    service    device/main//
    Register Entity    device/child0//    child-device    device/main//
    Register Entity    device/child00//    child-device    device/child0//
    Register Entity    device/child000//    child-device    device/child00//
    Register Entity    device/child1//    child-device    device/main//
    Register Entity    device/child1/service/service10    service    device/child1//
    Register Entity    device/child2//    child-device    device/main//
    Register Entity    device/child2/service/service20    service    device/child2//
    Register Entity    device/child2/service/service21    service    device/child2//
    Register Entity    device/child20//    child-device    device/child2//
    Register Entity    device/child21//    child-device    device/child2//
    Register Entity    device/child21/service/service210    service    device/child21//
    Register Entity    device/child210//    child-device    device/child21//
    Register Entity    device/child210/service/service2100    service    device/child210//
    Register Entity    device/child2100//    child-device    device/child210//
    Register Entity    device/child22//    child-device    device/child2//
