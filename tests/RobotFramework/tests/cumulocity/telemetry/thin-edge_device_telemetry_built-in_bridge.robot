*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs
Test Timeout        5 minutes

Test Tags           theme:c8y    theme:telemetry


*** Variables ***
${C8Y_TOPIC_PREFIX}     custom-c8y-prefix


*** Test Cases ***
Thin-edge devices support sending simple measurements
    Execute Command    tedge mqtt pub te/device/main///m/ '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=ThinEdgeMeasurement
    ...    value=temperature
    ...    series=temperature
    Log    ${measurements}

Built-in bridge reports health status
    Service Health Status Should Be Up    tedge-mapper-bridge-custom-c8y-prefix

Bridge stops if mapper stops running
    Execute Command    tedge mqtt pub ${C8Y_TOPIC_PREFIX}/s/us '200,CustomMeasurement,temperature,25'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=CustomMeasurement
    ...    series=temperature
    Log    ${measurements}
    Execute Command    systemctl stop tedge-mapper-c8y
    Service Health Status Should Be Down    tedge-mapper-bridge-custom-c8y-prefix
    Service Health Status Should Be Down    tedge-mapper-custom-c8y-prefix
    Execute Command    tedge mqtt pub ${C8Y_TOPIC_PREFIX}/s/us '200,CustomMeasurement,temperature,25'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=CustomMeasurement
    ...    series=temperature
    Log    ${measurements}
    Execute Command    systemctl start tedge-mapper-c8y

Thin-edge devices support sending simple measurements with custom type
    Execute Command    tedge mqtt pub te/device/main///m/ '{ "type":"CustomType", "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=CustomType
    ...    value=temperature
    ...    series=temperature
    Log    ${measurements}

Thin-edge devices support sending simple measurements with custom type in topic
    Execute Command    tedge mqtt pub te/device/main///m/CustomType_topic '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=CustomType_topic
    ...    value=temperature
    ...    series=temperature
    Log    ${measurements}

Thin-edge devices support sending simple measurements with custom type in payload
    Execute Command
    ...    tedge mqtt pub te/device/main///m/CustomType_topic '{ "type":"CustomType_payload","temperature": 25 }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=CustomType_payload
    ...    value=temperature
    ...    series=temperature
    Log    ${measurements}

Thin-edge devices support sending custom measurements
    Execute Command    tedge mqtt pub te/device/main///m/ '{ "current": {"L1": 9.5, "L2": 1.3} }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=ThinEdgeMeasurement
    ...    value=current
    ...    series=L1
    Log    ${measurements}

Thin-edge devices support sending custom events
    Execute Command
    ...    tedge mqtt pub te/device/main///e/myCustomType1 '{ "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s
    ...    expected_text=Some test event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=myCustomType1
    ...    fragment=someOtherCustomFragment
    Log    ${events}

Thin-edge devices support sending large events
    Execute Command
    ...    tedge mqtt pub te/device/main///e/largeEvent "$(printf '{"text":"Large event","large_text_field":"%s"}' "$(yes "x" | head -n 100000 | tr -d '\n')")"
    ${events}=    Device Should Have Event/s
    ...    expected_text=Large event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=largeEvent
    ...    fragment=large_text_field
    Length Should Be    ${events[0]["large_text_field"]}    100000
    Log    ${events}

Thin-edge devices support sending large events using legacy api
    [Tags]    legacy
    Execute Command
    ...    tedge mqtt pub tedge/events/largeEvent2 "$(printf '{"text":"Large event","large_text_field":"%s"}' "$(yes "x" | head -n 100000 | tr -d '\n')")"
    ${events}=    Device Should Have Event/s
    ...    expected_text=Large event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=largeEvent2
    ...    fragment=large_text_field
    Length Should Be    ${events[0]["large_text_field"]}    100000
    Log    ${events}

Thin-edge devices support sending custom events overriding the type
    Execute Command
    ...    tedge mqtt pub te/device/main///e/myCustomType '{"type": "otherType", "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s
    ...    expected_text=Some test event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=otherType
    ...    fragment=someOtherCustomFragment
    Log    ${events}

Thin-edge devices support sending custom events without type in topic
    Execute Command
    ...    tedge mqtt pub te/device/main///e/ '{"text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s
    ...    expected_text=Some test event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=ThinEdgeEvent
    ...    fragment=someOtherCustomFragment
    Log    ${events}

Thin-edge devices support sending custom alarms #1699
    [Tags]    \#1699
    Execute Command
    ...    tedge mqtt pub te/device/main///a/myCustomAlarmType '{ "severity": "critical", "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=Some test alarm
    ...    severity=CRITICAL
    ...    minimum=1
    ...    maximum=1
    ...    type=myCustomAlarmType
    Should Be Equal    ${alarms[0]["someOtherCustomFragment"]["nested"]["value"]}    extra info
    Log    ${alarms}

Thin-edge devices support sending custom alarms overriding the type
    Execute Command
    ...    tedge mqtt pub te/device/main///a/myCustomAlarmType '{ "severity": "critical", "text": "Some test alarm", "type": "otherType", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=Some test alarm
    ...    severity=CRITICAL
    ...    minimum=1
    ...    maximum=1
    ...    type=otherType
    Log    ${alarms}

Thin-edge devices support sending custom alarms without type in topic
    Execute Command
    ...    tedge mqtt pub te/device/main///a/ '{ "severity": "critical", "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=Some test alarm
    ...    severity=CRITICAL
    ...    minimum=1
    ...    maximum=1
    ...    type=ThinEdgeAlarm
    Log    ${alarms}

Thin-edge devices support sending custom alarms without severity in payload
    Execute Command    tedge mqtt pub te/device/main///a/myCustomAlarmType2 '{ "text": "Some test alarm" }'
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=Some test alarm
    ...    severity=MINOR
    ...    minimum=1
    ...    maximum=1
    ...    type=myCustomAlarmType2
    Log    ${alarms}

Thin-edge devices support sending custom alarms with unknown severity in payload
    Execute Command
    ...    tedge mqtt pub te/device/main///a/myCustomAlarmType3 '{ "severity": "invalid", "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=Some test alarm
    ...    severity=MINOR
    ...    minimum=1
    ...    maximum=1
    ...    type=myCustomAlarmType3
    Log    ${alarms}

Thin-edge devices support sending custom alarms without text in payload
    Execute Command
    ...    tedge mqtt pub te/device/main///a/myCustomAlarmType4 '{ "severity": "major", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=myCustomAlarmType4
    ...    severity=MAJOR
    ...    minimum=1
    ...    maximum=1
    ...    type=myCustomAlarmType4
    Log    ${alarms}

Thin-edge devices support sending alarms using text fragment
    Execute Command
    ...    tedge mqtt pub te/device/main///a/parentAlarmType1 '{ "severity": "minor", "text": "Some test alarm" }'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=Some test alarm
    ...    severity=MINOR
    ...    minimum=1
    ...    maximum=1
    ...    type=parentAlarmType1
    Log    ${alarms}

Thin-edge device supports sending custom Thin-edge device measurements directly to c8y
    Execute Command
    ...    tedge mqtt pub "${C8Y_TOPIC_PREFIX}/measurement/measurements/create" '{"time":"2023-03-20T08:03:56.940907Z","environment":{"temperature":{"value":29.9,"unit":"°C"}},"type":"10min_average","meta":{"sensorLocation":"Brisbane, Australia"}}'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    value=environment
    ...    series=temperature
    ...    type=10min_average
    Should Be Equal As Numbers    ${measurements[0]["environment"]["temperature"]["value"]}    29.9
    Should Be Equal    ${measurements[0]["meta"]["sensorLocation"]}    Brisbane, Australia
    Should Be Equal    ${measurements[0]["type"]}    10min_average

Thin-edge device supports sending custom bulk measurements directly to c8y
    Execute Command
    ...    tedge mqtt pub "${C8Y_TOPIC_PREFIX}/measurement/measurements/createBulk" '{"measurements":[{"time":"2024-12-01T02:00:00Z","outside":{"temperature":{"value":2.5,"unit":"°C"}},"type":"1min_average"},{"time":"2024-12-01T02:01:00Z","outside":{"temperature":{"value":3.5,"unit":"°C"}},"type":"1min_average"}]}'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${measurements}=    Device Should Have Measurements
    ...    minimum=2
    ...    maximum=2
    ...    value=outside
    ...    series=temperature
    ...    type=1min_average
    Should Be Equal As Numbers    ${measurements[0]["outside"]["temperature"]["value"]}    2.5
    Should Be Equal As Numbers    ${measurements[1]["outside"]["temperature"]["value"]}    3.5
    Should Be Equal    ${measurements[0]["type"]}    1min_average
    Should Be Equal    ${measurements[1]["type"]}    1min_average

Thin-edge device supports sending custom bulk events directly to c8y
    Execute Command
    ...    tedge mqtt pub "${C8Y_TOPIC_PREFIX}/event/events/createBulk" '{"events":[{"time":"2024-12-01T02:00:00Z","text":"event 1","type":"bulkevent"},{"time":"2024-12-01T02:01:00Z","text":"event 2","type":"bulkevent"}]}'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${events}=    Device Should Have Event/s
    ...    minimum=2
    ...    maximum=2
    ...    type=bulkevent
    Should Be Equal As Strings    ${events[0]["text"]}    event 2
    Should Be Equal As Strings    ${events[1]["text"]}    event 1
    Should Be Equal    ${events[0]["type"]}    bulkevent
    Should Be Equal    ${events[1]["type"]}    bulkevent

Thin-edge device supports sending custom bulk alarms directly to c8y
    Execute Command
    ...    tedge mqtt pub "${C8Y_TOPIC_PREFIX}/alarm/alarms/createBulk" '{"alarms":[{"time":"2024-12-01T02:00:00Z","text":"alarm 1","severity":"MAJOR","type":"bulkalarm1"},{"time":"2024-12-01T02:01:00Z","text":"alarm 2","severity":"MINOR","type":"bulkalarm2"}]}'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${alarms}=    Device Should Have Alarm/s
    ...    minimum=2
    ...    maximum=2
    ...    type=bulkalarm1,bulkalarm2
    Should Be Equal As Strings    ${alarms[0]["text"]}    alarm 2
    Should Be Equal As Strings    ${alarms[1]["text"]}    alarm 1
    Should Be Equal As Strings    ${alarms[0]["severity"]}    MINOR
    Should Be Equal As Strings    ${alarms[1]["severity"]}    MAJOR
    Should Be Equal    ${alarms[0]["type"]}    bulkalarm2
    Should Be Equal    ${alarms[1]["type"]}    bulkalarm1

Thin-edge device support sending inventory data via c8y topic
    Execute Command
    ...    tedge mqtt pub "${C8Y_TOPIC_PREFIX}/inventory/managedObjects/update/${DEVICE_SN}" '{"parentInfo":{"nested":{"name":"complex"}},"subType":"customType"}'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${mo}=    Device Should Have Fragments    parentInfo    subType
    Should Be Equal    ${mo["parentInfo"]["nested"]["name"]}    complex
    Should Be Equal    ${mo["subType"]}    customType

Previously cleared property should be sent to cloud when set again #2365
    [Tags]    \#2365
    Cumulocity.Set Device    ${DEVICE_SN}

    # set initial value
    Execute Command    tedge mqtt pub --retain "te/device/main///twin/subtype" '"LinuxDeviceA"'
    Device Should Have Fragment Values    subtype\=LinuxDeviceA

    # Clear
    Execute Command    tedge mqtt pub --retain "te/device/main///twin/subtype" ''
    Managed Object Should Not Have Fragments    subtype

    # Set to same value prior to clearing it
    Execute Command    tedge mqtt pub --retain "te/device/main///twin/subtype" '"LinuxDeviceA"'
    Device Should Have Fragment Values    subtype\=LinuxDeviceA

#
# Services
#
# measurements

Send measurements to an unregistered service
    Execute Command    tedge mqtt pub te/device/main/service/app1/m/service_type001 '{"temperature": 30.1}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}
    Cumulocity.Should Have Services    min_count=1    max_count=1    name=app1

    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:main:service:app1
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=service_type001
    Should Be Equal    ${measurements[0]["type"]}    service_type001
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    30.1

Send measurements to a registered service
    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/app2 '{"@type":"service","@parent":"device/main//"}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}
    Cumulocity.Should Have Services    name=app2    min_count=1    max_count=1

    Execute Command    tedge mqtt pub te/device/main/service/app2/m/service_type002 '{"temperature": 30.1}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:main:service:app2
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=service_type002
    Should Be Equal    ${measurements[0]["type"]}    service_type002
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    30.1

# alarms

Send alarms to an unregistered service
    Execute Command
    ...    tedge mqtt pub te/device/main/service/app3/a/alarm_001 '{"text": "test alarm","severity":"major"}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}
    Cumulocity.Should Have Services    min_count=1    max_count=1    name=app3

    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:main:service:app3
    ${alarms}=    Device Should Have Alarm/s    expected_text=test alarm    type=alarm_001    minimum=1    maximum=1
    Should Be Equal    ${alarms[0]["type"]}    alarm_001
    Should Be Equal    ${alarms[0]["severity"]}    MAJOR

Send alarms to a registered service
    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/app4 '{"@type":"service","@parent":"device/main//"}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}
    Cumulocity.Should Have Services    name=app4    min_count=1    max_count=1

    Execute Command    tedge mqtt pub te/device/main/service/app4/a/alarm_002 '{"text": "test alarm"}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:main:service:app4
    ${alarms}=    Device Should Have Alarm/s    expected_text=test alarm    type=alarm_002    minimum=1    maximum=1
    Should Be Equal    ${alarms[0]["type"]}    alarm_002

# events

Send events to an unregistered service
    Execute Command    tedge mqtt pub te/device/main/service/app5/e/event_001 '{"text": "test event"}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}
    Cumulocity.Should Have Services    name=app5    min_count=1    max_count=1

    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:main:service:app5
    Device Should Have Event/s    expected_text=test event    type=event_001    minimum=1    maximum=1

Send events to a registered service
    Execute Command
    ...    tedge mqtt pub --retain te/device/main/service/app6 '{"@type":"service","@parent":"device/main//"}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}
    Cumulocity.Should Have Services    name=app6    min_count=1    max_count=1

    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:main:service:app6
    Execute Command    tedge mqtt pub te/device/main/service/app6/e/event_002 '{"text": "test event"}'
    Device Should Have Event/s    expected_text=test event    type=event_002    minimum=1    maximum=1


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Execute Command    tedge config set mqtt.bridge.built_in true
    ThinEdgeIO.Execute Command    tedge config set c8y.bridge.topic_prefix custom-c8y-prefix
    ThinEdgeIO.Execute Command    tedge reconnect c8y
    Service Health Status Should Be Up    tedge-mapper-custom-c8y-prefix
    ${output}=    Execute Command    sudo tedge connect c8y --test    stdout=${False}    stderr=${True}
    Should Contain    ${output}    Connection check to c8y cloud is successful.
