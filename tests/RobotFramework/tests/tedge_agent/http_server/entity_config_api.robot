*** Settings ***
Documentation       Verify that tedge-agent publishes its own exposable configuration as retained
...                 MQTT messages under its own service topic, and serves them over the
...                 GET /te/v1/entities/<service>/config[/<key>] HTTP routes, while secret settings
...                 never appear on either surface.

Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs    ${DEVICE_SN}

Test Tags           theme:tedge_agent


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Main device serial number


*** Test Cases ***
Agent publishes an exposed core setting as a retained MQTT message
    ${device_id}=    Execute Command    tedge config get device.id    strip=${True}
    ${retained}=    Execute Command
    ...    tedge mqtt sub te/device/main/service/tedge-agent/config/device.id --retained-only --no-topic --duration 1s
    ...    strip=${True}
    Should Be Equal    ${retained}    ${device_id}

Agent serves a single exposed value over HTTP
    ${device_id}=    Execute Command    tedge config get device.id    strip=${True}
    ${get}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config/device.id
    Should Be Equal    ${get}    ${device_id}|200

Agent serves the whole exposed config as a JSON object over HTTP
    ${device_id}=    Execute Command    tedge config get device.id    strip=${True}
    ${mqtt_port}=    Execute Command    tedge config get mqtt.client.port    strip=${True}
    ${get}=    Execute Command
    ...    curl --silent http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config
    Should Contain    ${get}    "device.id":"${device_id}"
    Should Contain    ${get}    "mqtt.client.port":"${mqtt_port}"

A non-exposed secret setting never appears on the retained config topic
    Should Not Have Retained MQTT Messages
    ...    topic=te/device/main/service/tedge-agent/config/device.key_pin

A non-exposed secret setting never appears in the HTTP config view
    ${get}=    Execute Command
    ...    curl --silent http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config
    Should Not Contain    ${get}    key_pin

A non-exposed key returns 404, indistinguishable from an unknown key
    ${secret}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -o /dev/null http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config/device.key_pin
    Should Be Equal    ${secret}    404

    ${unknown}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -o /dev/null http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config/no.such.key
    Should Be Equal    ${unknown}    404

The config HTTP view rejects writes
    ${put}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -o /dev/null -X PUT http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config/device.id -d 'other-value'
    Should Be Equal    ${put}    405

    ${delete}=    Execute Command
    ...    curl --silent --write-out "%\{http_code\}" -o /dev/null -X DELETE http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config/device.id
    Should Be Equal    ${delete}    405


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
