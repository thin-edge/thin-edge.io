*** Settings ***
Documentation       Verify that a bootstrapped c8y mapper publishes its own exposable cloud
...                 configuration as one retained MQTT JSON message under its own service topic, with
...                 the cloud qualifier stripped from each key, and that the agent serves them over the
...                 GET /te/v1/entities/<service>/config[/<key>] HTTP routes.

Resource            ../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO
Library             JSONLibrary

Suite Setup         Custom Setup
Test Teardown       Get Logs    ${DEVICE_SN}

Test Tags           theme:c8y


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Main device serial number


*** Test Cases ***
Mapper publishes its own settings with the cloud qualifier stripped as one retained JSON message
    ${url}=    Execute Command    tedge config get c8y.url    strip=${True}
    ${topic_prefix}=    Execute Command    tedge config get c8y.bridge.topic_prefix    strip=${True}

    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-mapper-c8y/config

    ${config}=    JSONLibrary.Convert String To Json    ${retained}[0]
    Should Be Equal As Strings    ${config["url"]}    ${url}
    Should Be Equal As Strings    ${config["bridge.topic_prefix"]}    ${topic_prefix}

Mapper does not publish another cloud's settings
    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-mapper-c8y/config
    Should Not Contain    ${retained}[0]    az.url
    Should Not Contain    ${retained}[0]    aws.url

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
    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-mapper-c8y/config
    Should Not Contain    ${retained}[0]    key_pin

A non-exposed c8y secret setting never appears in the HTTP config view
    ${get}=    Execute Command
    ...    curl --silent http://localhost:8000/te/v1/entities/device/main/service/tedge-mapper-c8y/config
    Should Not Contain    ${get}    key_pin
    Should Not Contain    ${get}    credentials_path


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
