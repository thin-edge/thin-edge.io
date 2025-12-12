*** Settings ***
Documentation       Purpose of this test is to verify that tedge-mapper-az translates the messages that arrive on te topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:az


*** Test Cases ***
Publish measurements to te measurement topic with measurement type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///m/test-type '{"pressure": 10}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="pressure"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish measurements to te measurement topic without measurement type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///m/ '{"windspeed": 10}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="windspeed"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish service measurements to te measurement topic with measurement type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main/service/test_service/m/test-type '{"temp": 10}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="temp"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish child measurements to te measurement topic with measurement type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/child///m/test-type '{"temperature_child": 10}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="temperature_child"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish main device event to te event topic with event type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///e/event-type '{"text": "someone logged-in"}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="someone logged-in"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish main device event to te event topic without event type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///e/ '{"text": "someone logged-off"}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="someone logged-off"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish child device event to te event topic with event type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/child///e/event-type '{"text": "child_device event"}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="child_device event"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish main device alarm to te alarm topic with alarm type
    ${timestamp}=    Get Unix Timestamp
    Execute Command
    ...    tedge mqtt pub te/device/main///a/alarm-type '{"severity":"critical","text": "someone logged-in"}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="critical"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish main device alarm to te alarm topic without alarm type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///a/ '{"severity":"major","text": "someone logged-in"}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="major"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish child device alarm to te alarm topic with alarm type
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/child///a/alarm-type '{"severity":"minor","text": "someone logged-in"}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="minor"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Publish health status message for main device service
    Execute Command    tedge mqtt pub te/device/main/service/test-service/status/health '{"status":"up"}'
    Should Have MQTT Messages    az/messages/events/    message_contains="status":"up"

Discard messages that are too large
    ${timestamp}=    Get Unix Timestamp
    ${event}=    Execute Command    for i in $(seq 100); do echo -n 1234; done
    Execute Command    tedge mqtt pub te/device/main///e/event-type '{"text": "some digits: ${event}"}'
    Should Have MQTT Messages
    ...    te/errors
    ...    message_contains=Payload is too large
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Discard mosquitto health status
    Execute Command    (sleep 1; tedge mqtt pub te/device/main/service/mosquitto-c8y-bridge/status/health 1)&
    ${message}=    Execute Command
    ...    tedge mqtt sub 'az/#' --duration 2s | grep -v '"status":"up"'
    ...    ignore_exit_code=${True}
    Should Be Empty    ${message}


*** Keywords ***
Custom Setup
    Setup
    Execute Command    tedge config set mqtt.bridge.built_in false
    Execute Command    tedge config set az.mapper.mqtt.max_payload_size 128
    Execute Command    sudo systemctl restart tedge-mapper-az.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-az

Custom Teardown
    Get Suite Logs
    Execute Command    sudo tedge config unset az.topics
