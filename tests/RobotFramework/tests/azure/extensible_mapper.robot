*** Settings ***
Documentation       Purpose of this test is to verify that tedge-mapper-az translates the messages that arrive on te topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:az


*** Test Cases ***
Legacy behavior
    # The legacy az mapper adds a timestamp if missing but ignores message source and measurement type
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/child-az///m/env '{"temperature":10, "time":1768404563.338 }'
    ${message}    Should Have MQTT Messages
    ...    topic=az/messages/events/
    ...    message_contains=temperature
    ...    date_from=${start}
    Should Contain    ${message}[0]    "time":1768404563.338
    Should Not Contain    ${message}[0]    "type":
    Should Not Contain    ${message}[0]    "source":

Updating the builtin flow definition
    ${start}    Get Unix Timestamp
    Execute Command    sed -i -e s/unix/rfc3339/g /etc/tedge/mappers/az/flows/mea.toml
    Wait For The Flow To Reload    ${start}    mea.toml

    # Now the az mapper reformats the timestamps using rfc3339
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/child-az///m/env '{"temperature":10, "time":1768404563.338 }'
    ${message}    Should Have MQTT Messages
    ...    topic=az/messages/events/
    ...    message_contains=temperature
    ...    date_from=${start}
    Should Contain    ${message}[0]    "time":"2026-01-14T15:29:23.338Z"
    Should Not Contain    ${message}[0]    "type":
    Should Not Contain    ${message}[0]    "source":

Adding a step to the builtin flow definition
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-az.js    /etc/tedge/mappers/az/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mea-v2.toml    /etc/tedge/mappers/az/flows/mea.toml
    Wait For The Flow To Reload    ${start}    mea.toml

    # Thanks to the custom script the az mapper adds the measurement type
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/child-az///m/env '{"temperature":10, "time":1768404563.338 }'
    ${message}    Should Have MQTT Messages
    ...    topic=az/messages/events/
    ...    message_contains=temperature
    ...    date_from=${start}
    Should Contain    ${message}[0]    "time":"2026-01-14T15:29:23.338Z"
    Should Contain    ${message}[0]    "type":"env"
    Should Not Contain    ${message}[0]    "source":

Adding a flow to the builtin mapper
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/az-registration.js    /etc/tedge/mappers/az/flows/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/az-registration.toml    /etc/tedge/mappers/az/flows/
    Wait For The Flow To Reload    ${start}    az-registration.toml

    # Thanks to the registration flow, the az mapper now processes entity registration messages
    # The registration messages being stored in the mapper context
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub -r te/device/child-az// '{"name": "Azure-Child"}'
    Should Have MQTT Messages    topic=te/infos    message_contains=New entity: Azure-Child    date_from=${start}

    # The source names can now be extrated from the mapper context
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-az-v2.js    /etc/tedge/mappers/az/flows/custom-az.js
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mea-v2.toml    /etc/tedge/mappers/az/flows/mea.toml
    Wait For The Flow To Reload    ${start}    mea.toml

    # So the az mapper adds the source name to measurements
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/child-az///m/env '{"temperature":10, "time":1768404563.338 }'
    ${message}    Should Have MQTT Messages
    ...    topic=az/messages/events/
    ...    message_contains=temperature
    ...    date_from=${start}
    Should Contain    ${message}[0]    "time":"2026-01-14T15:29:23.338Z"
    Should Contain    ${message}[0]    "type":"env"
    Should Contain    ${message}[0]    "source":"Azure-Child"


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

Wait For The Flow To Reload
    [Arguments]    ${start}    ${flow}
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-az/status/flows
    ...    date_from=${start}
    ...    message_contains=${flow}
