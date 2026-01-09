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

Reformat timestamps
    # By default the az mapper reformat timestamps which format is not the configured one
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///m/ '{"time":"2025-12-16T09:37:55.135+00:00"}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="time":1765877875.135
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1

Update builtin az mapper flows
    # Update the az mapper so message timestamps are not reformated
    Update Builtin Flow    updated_mea.toml
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///m/ '{"time":"2025-12-16T09:37:55.135+00:00"}'
    Should Have MQTT Messages
    ...    az/messages/events/#
    ...    message_contains="time":"2025-12-16T09:37:55.135+00:00"
    ...    date_from=${timestamp}
    ...    minimum=1
    ...    maximum=1
    [Teardown]    Restore Builtin Flow

Updated builtin flows should not be overridden by the az mapper
    # By default the az mapper reformat timestamps which format is not the configured one
    Execute Command    cmd=grep 'reformat = true' /etc/tedge/mappers/az/flows/mea.toml
    # Update the az mapper so message timestamps are not reformated
    Update Builtin Flow    updated_mea.toml
    # Restarting the mapper should not override the user-provided flow
    Execute Command    sudo systemctl restart tedge-mapper-az.service
    Execute Command    cmd=grep 'reformat = false' /etc/tedge/mappers/az/flows/mea.toml
    [Teardown]    Restore Builtin Flow

Disable builtin az mapper flows
    Disable Builtin Flow
    Execute Command    (sleep 1; tedge mqtt pub te/device/main///e/ '{"text": "az builtin flow as been turned off"}')&
    ${message}=    Execute Command
    ...    tedge mqtt sub 'az/#' --duration 2s
    ...    ignore_exit_code=${True}
    Should Be Empty    ${message}
    [Teardown]    Restore Builtin Flow

Monitor flow definition updates
    ${start}=    Get Unix Timestamp
    Execute Command    touch /etc/tedge/mappers/az/flows/mea.toml
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-az/status/flows
    ...    date_from=${start}
    ...    message_contains=mea.toml


*** Keywords ***
Custom Setup
    Setup
    Execute Command    tedge config set mqtt.bridge.built_in false
    Execute Command    tedge config set az.mapper.timestamp_format unix
    Execute Command    tedge config set az.mapper.mqtt.max_payload_size 128
    Execute Command    sudo systemctl restart tedge-mapper-az.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-az

Custom Teardown
    Get Suite Logs
    Execute Command    sudo tedge config unset az.topics

Update Builtin Flow
    [Arguments]    ${flow_definition}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/${flow_definition}    /etc/tedge/mappers/az/flows/mea.toml

Disable Builtin Flow
    Execute Command    mv /etc/tedge/mappers/az/flows/mea.toml /etc/tedge/mappers/az/flows/mea.toml.disabled

Restore Builtin Flow
    Execute Command    cp /etc/tedge/mappers/az/flows/mea.toml.template /etc/tedge/mappers/az/flows/mea.toml
    Execute Command    rm -f /etc/tedge/mappers/az/flows/mea.toml.disabled
