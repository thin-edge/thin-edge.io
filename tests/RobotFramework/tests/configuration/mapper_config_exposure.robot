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

Test Tags           theme:c8y    theme:configuration


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Main device serial number


*** Test Cases ***
Mapper publishes its own settings with the cloud qualifier stripped as one retained JSON message
    [Documentation]    Each value keeps the type it has in tedge.toml: a capability flag is
    ...    published as a JSON boolean and a SmartREST template set as a JSON array, rather than
    ...    as the string renderings of those values.
    ${url}=    Execute Command    tedge config get c8y.url    strip=${True}
    ${topic_prefix}=    Execute Command    tedge config get c8y.bridge.topic_prefix    strip=${True}
    ${max_payload_size}=    Execute Command    tedge config get c8y.mapper.mqtt.max_payload_size    strip=${True}

    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-mapper-c8y/config

    ${config}=    JSONLibrary.Convert String To Json    ${retained}[0]
    Should Be Equal As Strings    ${config["url"]}    ${url}
    Should Be Equal As Strings    ${config["bridge.topic_prefix"]}    ${topic_prefix}
    Should Be Equal    ${config["enable.log_upload"]}    ${True}
    Should Be Equal As Strings    ${config["bridge.topic_prefix"]}    ${topic_prefix}
    Should Be Equal As Integers    ${config["mapper.mqtt.max_payload_size"]}    ${max_payload_size}

Mapper does not publish another cloud's settings
    Execute Command    tedge config set az.url test.azure.com
    Execute Command    tedge config set aws.url test.aws.com

    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-mapper-c8y/config
    Should Not Contain    ${retained}[0]    test.azure.com
    Should Not Contain    ${retained}[0]    test.aws.com

Agent serves the mapper's single exposed value over HTTP
    ${url}=    Execute Command    tedge config get c8y.url    strip=${True}
    ${get}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" http://localhost:8000/te/v1/entities/device/main/service/tedge-mapper-c8y/config/url
    Should Be Equal    ${get}    "${url}"|200

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

Mapper republishes its config after an external client overwrites it
    [Documentation]    If a third party publishes a bogus retained message onto the mapper's own
    ...    config topic, the mapper notices the mismatch against its own known exposed values and
    ...    republishes the correct document, self-healing the retained state.
    ${url}=    Execute Command    tedge config get c8y.url    strip=${True}

    ${start}=    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/main/service/tedge-mapper-c8y/config' '{"bad":"config"}'

    # The mapper notices the retained payload no longer matches its own config and republishes it
    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-c8y/config
    ...    minimum=1
    ...    date_from=${start}
    ...    message_contains="url":"${url}"

    # The corrected document, not the injected one, is what ends up retained
    ${retained}=    Should Have Retained MQTT Messages
    ...    te/device/main/service/tedge-mapper-c8y/config
    Should Not Contain    ${retained}[0]    "bad":"config"
    ${config}=    JSONLibrary.Convert String To Json    ${retained}[0]
    Should Be Equal As Strings    ${config["url"]}    ${url}

Config of a profiled c8y mapper is exposed under its own service topic
    [Documentation]    A c8y mapper connected under a cloud profile publishes its exposed config
    ...    under a service topic derived from the profile's own bridge.topic_prefix, using the
    ...    profile-qualified settings rather than the default profile's.
    [Setup]    Switch Main Device To A c8y Profile    test

    ${url}=    Execute Command    tedge config get c8y.url --profile test    strip=${True}

    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-mapper-c8y-test/config
    ${config}=    JSONLibrary.Convert String To Json    ${retained}[0]
    Should Be Equal As Strings    ${config["url"]}    ${url}
    Should Be Equal As Strings    ${config["bridge.topic_prefix"]}    c8y-test

    ${get}=    Execute Command
    ...    curl --silent --write-out "|%\{http_code\}" http://localhost:8000/te/v1/entities/device/main/service/tedge-mapper-c8y-test/config/url
    Should Be Equal    ${get}    "${url}"|200

    [Teardown]    Restore Main Device From c8y Profile    test

Non-cloud custom mapper still publishes an empty exposed config document
    [Documentation]    tedge-mapper-local is a built-in custom (non-cloud) mapper. It has no
    ...    cloud settings to expose, but the generic config-publisher actor runs for every mapper,
    ...    so it still publishes an empty retained JSON object, which the agent still serves over
    ...    HTTP the same way it does for a cloud mapper's non-empty config.
    [Setup]    Start Service    tedge-mapper-local

    ${retained}=    Should Have Retained MQTT Messages    te/device/main/service/tedge-mapper-local/config
    Should Be Equal As Strings    ${retained}[0]    {}

    [Teardown]    Stop Service    tedge-mapper-local


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN

Restore The Default c8y Templates And Flags
    Execute Command    tedge config unset c8y.enable.log_upload
    Execute Command    tedge config unset c8y.smartrest.templates
    Restart Service    tedge-mapper-c8y

Switch Main Device To A c8y Profile
    [Arguments]    ${profile}
    Execute Command    tedge disconnect c8y
    Execute Command    sudo mv /etc/tedge/mappers/c8y /etc/tedge/mappers/c8y.${profile}
    Execute Command    sudo tedge config set c8y.bridge.topic_prefix --profile ${profile} c8y-${profile}
    Execute Command    tedge connect c8y --profile ${profile}    timeout=0

Restore Main Device From c8y Profile
    [Arguments]    ${profile}
    Execute Command    tedge disconnect c8y --profile ${profile}
    Execute Command    sudo mv /etc/tedge/mappers/c8y.${profile} /etc/tedge/mappers/c8y
    Execute Command    tedge connect c8y    timeout=0
