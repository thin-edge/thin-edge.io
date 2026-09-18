*** Settings ***
Documentation       Verify that Azure IoT Hub direct methods are turned into thin-edge commands,
...                 and command outcomes are turned back into direct method responses,
...                 entirely via the direct-methods flows (no cloud connection required).

Resource            ../../../../resources/common.resource
Library             String
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:mqtt    theme:az


*** Test Cases ***
Happy path
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'az/methods/POST/shell/?$rid\=42' '{"command":"echo hello"}'
    ${message}    Should Have MQTT Messages
    ...    topic=az/methods/res/200/?$rid=42
    ...    date_from=${start}
    Should Contain    ${message}[0]    hello
    Should Not Have Retained MQTT Messages    te/device/main///cmd/shell_execute/az-dm-42

Failing command
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'az/methods/POST/shell/?$rid\=43' '{"command":"exit 3"}'
    ${message}    Should Have MQTT Messages
    ...    topic=az/methods/res/500/?$rid=43
    ...    date_from=${start}
    Should Contain    ${message}[0]    "exitCode":3
    Should Contain    ${message}[0]    reason
    Should Not Have Retained MQTT Messages    te/device/main///cmd/shell_execute/az-dm-43

Unmapped method uses the method name as the command name
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'az/methods/POST/shell_execute/?$rid\=44' '{"command":"echo hello"}'
    ${message}    Should Have MQTT Messages
    ...    topic=az/methods/res/200/?$rid=44
    ...    date_from=${start}
    Should Contain    ${message}[0]    hello
    Should Not Have Retained MQTT Messages    te/device/main///cmd/shell_execute/az-dm-44

Rejected request is answered with a 400
    [Documentation]    This stands for every request the flow rejects before
    ...    triggering a command: an unusable method name or $rid, a payload that
    ...    is not a JSON object, and, as here, one missing a required field.
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'az/methods/POST/shell/?$rid\=45' '{}'
    Should Have MQTT Messages
    ...    topic=az/methods/res/400/?$rid=45
    ...    date_from=${start}
    Should Not Have MQTT Messages
    ...    topic=te/+/+/+/+/cmd/+/+
    ...    date_from=${start}

Request with no $rid is only reported on te/errors
    [Documentation]    Without a $rid there is no response topic to answer on,
    ...    so such a request can only be reported on te/errors.
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'az/methods/POST/shell/?foo\=bar' '{"command":"echo hello"}'
    Should Not Have MQTT Messages
    ...    topic=az/methods/res/#
    ...    date_from=${start}
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

    Transfer To Device    ${CURDIR}/flows/*    /etc/tedge/mappers/az/flows/direct-methods/
    Execute Command    chown -R tedge:tedge /etc/tedge/mappers/az/flows

    Execute Command    sudo systemctl restart tedge-mapper-az.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-az

    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-az/status/flows
    ...    message_contains=direct-methods
