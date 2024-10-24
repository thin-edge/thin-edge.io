*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Test Cases ***
Run shell custom operation for main device and publish the status
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command_1    /etc/tedge/operations/c8y/c8y_Command
    ${operation}=    Cumulocity.Create Operation
    ...    description=echo helloworld
    ...    fragments={"c8y_Command":{"text":"echo helloworld"}}

    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    helloworld\n
    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_pattern=^(504|505|506),[0-9]+($|,\\"helloworld\n\\")
    ...    minimum=2
    ...    maximum=2

Run shell custom operation for main device and do not publish the status
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command_2    /etc/tedge/operations/c8y/c8y_Command
    Restart Service    tedge-mapper-c8y
    ${operation}=    Cumulocity.Create Operation
    ...    description=echo helloworld
    ...    fragments={"c8y_Command":{"text":"echo helloworld"}}

    Operation Should Be PENDING    ${operation}

    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_pattern=^(504|505|506),[0-9]+($|,\\"helloworld\n\\")
    ...    minimum=0
    ...    maximum=0

Run arbitrary shell command
    # See https://github.com/thin-edge/thin-edge.io/issues/3186
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command_3    /etc/tedge/operations/c8y/c8y_Command
    ${operation}=    Cumulocity.Create Operation
    ...    description=mqtt pub hello world
    ...    fragments={"c8y_MqttPub":{"topic":"test-topic", "message": "hello world"}}
    Should Have MQTT Messages
    ...    test-topic
    ...    message_pattern=hello world
    Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/command_handler.*    /etc/tedge/operations/command
    Execute Command    chmod a+x /etc/tedge/operations/command
