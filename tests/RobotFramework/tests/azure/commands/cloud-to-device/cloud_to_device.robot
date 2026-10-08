*** Settings ***
Documentation       Verify that Azure IoT Hub cloud-to-device messages are turned into thin-edge
...                 commands, entirely via the cloud-to-device flow (no cloud connection required).
...                 Unlike direct methods, C2D messages expect no response: commands they trigger
...                 are simply cleared once they reach a terminal state (see commands/flows/response.js).

Resource            ../../../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:mqtt    theme:az


*** Test Cases ***
Happy path with $.mid
    ${start}    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'az/messages/devicebound/method\=shell&$.mid\=test-mid-1' '{"command":"echo hello"}'
    ${message}    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/shell_execute/az-c2d-test-mid-1
    ...    message_contains="status":"successful"
    ...    date_from=${start}
    Should Contain    ${message}[0]    hello
    Should Not Have MQTT Messages
    ...    topic=az/methods/res/#
    ...    date_from=${start}
    Wait Until Keyword Succeeds
    ...    10x    1s
    ...    Should Not Have Retained MQTT Messages    te/device/main///cmd/shell_execute/az-c2d-test-mid-1

Happy path without $.mid falls back to a timestamp-based command id
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'az/messages/devicebound/method\=shell' '{"command":"echo hello"}'
    ${message}    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/shell_execute/+
    ...    message_contains="status":"successful"
    ...    date_from=${start}
    Should Contain    ${message}[0]    hello
    Should Not Have MQTT Messages
    ...    topic=az/methods/res/#
    ...    date_from=${start}
    Wait Until Keyword Succeeds
    ...    10x    1s
    ...    Should Not Have Retained MQTT Messages    te/device/main///cmd/shell_execute/+

Unmapped method uses the method name as the command name
    ${start}    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub 'az/messages/devicebound/method\=shell_execute&$.mid\=test-mid-2' '{"command":"echo hello"}'
    ${message}    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/shell_execute/az-c2d-test-mid-2
    ...    message_contains="status":"successful"
    ...    date_from=${start}
    Should Contain    ${message}[0]    hello
    Wait Until Keyword Succeeds
    ...    10x    1s
    ...    Should Not Have Retained MQTT Messages    te/device/main///cmd/shell_execute/az-c2d-test-mid-2

Malformed payload is only reported on te/errors
    [Documentation]    A C2D message expects no response, so a rejected message
    ...    leaves no trace but a te/errors entry. This stands for every rejection
    ...    the flow can raise: an unusable method name or $.mid, a payload that is
    ...    not a JSON object, or a message with no method property at all.
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'az/messages/devicebound/method\=shell' '{}'
    Should Not Have MQTT Messages
    ...    topic=te/+/+/+/+/cmd/+/+
    ...    date_from=${start}
    Should Have MQTT Messages
    ...    topic=te/errors
    ...    date_from=${start}


*** Keywords ***
Custom Setup
    Setup
    Execute Command    tedge config set mqtt.bridge.built_in false
    Transfer To Device    ${CURDIR}/../workflows/shell_execute.toml    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/../workflows/shell_execute.sh    /etc/tedge/operations/
    Execute Command    chmod a+x /etc/tedge/operations/shell_execute.sh

    Transfer To Device    ${CURDIR}/flows/*    /etc/tedge/mappers/az/flows/cloud-to-device/
    # response.js/.toml is shared with the direct-methods flow (see commands/flows/response.js):
    # it clears any thin-edge command once it reaches a terminal state. Unlike
    # direct-methods-triggered commands, C2D-triggered commands are only ever cleared,
    # never answered.
    Transfer To Device    ${CURDIR}/../flows/response.*    /etc/tedge/mappers/az/flows/cloud-to-device/
    Execute Command    chown -R tedge:tedge /etc/tedge/mappers/az/flows

    Execute Command    sudo systemctl restart tedge-mapper-az.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-az

    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-az/status/flows
    ...    message_contains=cloud-to-device
