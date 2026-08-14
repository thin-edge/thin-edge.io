*** Settings ***
Documentation       Verify that tedge-agent publishes its own exposable configuration as one retained
...                 MQTT JSON message under its own service topic, and serves them over the
...                 GET /te/v1/entities/<service>/config[/<key>] HTTP routes, while secret settings
...                 never appear on either surface.

Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO
Library             JSONLibrary

Suite Setup         Custom Setup
Test Teardown       Get Logs    ${DEVICE_SN}

Test Tags           theme:tedge_agent    theme:configuration


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Main device serial number
${DEVICE_ID}    ${EMPTY}    # Device ID from config
${MQTT_PORT}    ${EMPTY}    # MQTT client port from config


*** Test Cases ***
Agent publishes its exposed core settings as one retained MQTT JSON message
    [Documentation]    Each value keeps the type it has in tedge.toml: a port is published as a
    ...    JSON number and a capability flag as a JSON boolean, rather than as the string
    ...    renderings of those values.
    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-agent/config

    ${config}=    JSONLibrary.Convert String To Json    ${retained}[0]
    Should Be Equal As Strings    ${config["device.id"]}    ${DEVICE_ID}
    Should Be Equal As Integers    ${config["mqtt.client.port"]}    ${MQTT_PORT}
    Should Contain    ${retained}[0]    "mqtt.client.port":${MQTT_PORT}
    Should Match Regexp    ${retained}[0]    "agent\\.entity_store\\.auto_register":(true|false)

Agent serves a single exposed value over HTTP
    [Documentation]    The single-key route responds with JSON, so a string setting comes back
    ...    quoted and a numeric setting comes back unquoted.
    ${get}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config/device.id
    Should Be Equal    ${get}    "${DEVICE_ID}"|200

    ${get}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config/mqtt.client.port
    Should Be Equal    ${get}    ${MQTT_PORT}|200

Agent serves the whole exposed config as a JSON object over HTTP
    ${get}=    Execute Command
    ...    curl --silent http://localhost:8000/te/v1/entities/device/main/service/tedge-agent/config
    Should Contain    ${get}    "device.id":"${DEVICE_ID}"
    Should Contain    ${get}    "mqtt.client.port":${MQTT_PORT}

A non-exposed secret setting never appears on the retained config topic
    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-agent/config
    Should Not Contain    ${retained}[0]    key_pin

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
    ${device_id}=    Execute Command    tedge config get device.id    strip=${True}
    Set Suite Variable    ${DEVICE_ID}    ${device_id}
    ${mqtt_port}=    Execute Command    tedge config get mqtt.client.port    strip=${True}
    Set Suite Variable    ${MQTT_PORT}    ${mqtt_port}
