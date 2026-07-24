*** Settings ***
Documentation       Verify that a bootstrapped c8y mapper publishes its own exposable cloud
...                 configuration as retained MQTT messages under its own service topic, with the
...                 cloud qualifier stripped from the key, and that the agent serves them over the
...                 GET /te/v1/entities/<service>/config[/<key>] HTTP routes.

Resource            ../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs    ${DEVICE_SN}

Test Tags           theme:c8y


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Main device serial number


*** Test Cases ***
Mapper publishes its own url setting with the cloud qualifier stripped
    ${url}=    Execute Command    tedge config get c8y.url    strip=${True}
    ${retained}=    Execute Command
    ...    tedge mqtt sub te/device/main/service/tedge-mapper-c8y/config/url --retained-only --no-topic --duration 1s
    ...    strip=${True}
    Should Be Equal    ${retained}    ${url}

Mapper does not publish another cloud's settings
    Should Not Have Retained MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-c8y/config/az.url
    Should Not Have Retained MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-c8y/config/aws.url

Agent serves the mapper's single exposed value over HTTP
    ${url}=    Execute Command    tedge config get c8y.url    strip=${True}
    ${get}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" http://localhost:8000/te/v1/entities/device/main/service/tedge-mapper-c8y/config/url
    Should Be Equal    ${get}    ${url}|200

Agent serves the mapper's whole exposed config as a JSON object over HTTP
    ${url}=    Execute Command    tedge config get c8y.url    strip=${True}
    ${topic_prefix}=    Execute Command    tedge config get c8y.bridge.topic_prefix    strip=${True}
    ${get}=    Execute Command
    ...    curl --silent http://localhost:8000/te/v1/entities/device/main/service/tedge-mapper-c8y/config
    Should Contain    ${get}    "url":"${url}"
    Should Contain    ${get}    "bridge.topic_prefix":"${topic_prefix}"

A non-exposed c8y secret setting never appears on the retained config topic
    Should Not Have Retained MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-c8y/config/device.key_pin

A non-exposed c8y secret setting never appears in the HTTP config view
    ${get}=    Execute Command
    ...    curl --silent http://localhost:8000/te/v1/entities/device/main/service/tedge-mapper-c8y/config
    Should Not Contain    ${get}    key_pin
    Should Not Contain    ${get}    credentials_path


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
