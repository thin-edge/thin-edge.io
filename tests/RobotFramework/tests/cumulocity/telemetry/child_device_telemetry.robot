*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:telemetry


*** Test Cases ***
Child devices support sending simple measurements
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///m/ '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=ThinEdgeMeasurement
    ...    value=temperature
    ...    series=temperature
    Log    ${measurements}

Child devices support sending simple measurements with custom type in topic
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///m/CustomType_topic '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=CustomType_topic
    ...    value=temperature
    ...    series=temperature
    Log    ${measurements}

Child devices support sending simple measurements with custom type in payload
    Execute Command
    ...    tedge mqtt pub te/device/${CHILD_SN}///m/CustomType_topic '{ "type":"CustomType_payload","temperature": 25 }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=CustomType_payload
    ...    value=temperature
    ...    series=temperature
    Log    ${measurements}

Child devices support sending custom measurements
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///m/ '{ "current": {"L1": 9.5, "L2": 1.3} }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=ThinEdgeMeasurement
    ...    value=current
    ...    series=L1
    Log    ${measurements}

Child devices support sending measurements with units
    Execute Command    tedge mqtt pub -r te/device/${CHILD_SN}///m/child-mea/meta '{ "temperature": { "unit": "°C" } }'
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///m/child-mea '{ "temperature": 25.123 }'
    ${measurements}=    Device Should Have Measurements
    ...    minimum=1
    ...    maximum=1
    ...    type=child-mea
    ...    value=temperature
    ...    series=temperature
    Log    ${measurements}
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    25.123
    Should Be Equal    ${measurements[0]["temperature"]["temperature"]["unit"]}    °C

Child devices support sending custom events
    Execute Command
    ...    tedge mqtt pub te/device/${CHILD_SN}///e/myCustomType1 '{ "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s
    ...    expected_text=Some test event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=myCustomType1
    ...    fragment=someOtherCustomFragment
    Log    ${events}

Child devices support sending custom events overriding the type
    Execute Command
    ...    tedge mqtt pub te/device/${CHILD_SN}///e/myCustomType '{"type": "otherType", "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s
    ...    expected_text=Some test event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=otherType
    ...    fragment=someOtherCustomFragment
    Log    ${events}

 Child devices support sending custom events without type in topic
    Execute Command
    ...    tedge mqtt pub te/device/${CHILD_SN}///e/ '{"text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s
    ...    expected_text=Some test event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=ThinEdgeEvent
    ...    fragment=someOtherCustomFragment
    Log    ${events}

Child devices support sending large events
    Execute Command
    ...    tedge mqtt pub te/device/${CHILD_SN}///e/largeEvent "$(printf '{"text":"Large event","large_text_field":"%s"}' "$(yes "x" | head -n 100000 | tr -d '\n')")"
    ${events}=    Device Should Have Event/s
    ...    expected_text=Large event
    ...    with_attachment=False
    ...    minimum=1
    ...    maximum=1
    ...    type=largeEvent
    ...    fragment=large_text_field
    Length Should Be    ${events[0]["large_text_field"]}    100000
    Log    ${events}

Child devices support sending custom alarms #1699
    [Tags]    \#1699
    Execute Command
    ...    tedge mqtt pub te/device/${CHILD_SN}///a/myCustomAlarmType '{ "severity": "critical", "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=Some test alarm
    ...    severity=CRITICAL
    ...    minimum=1
    ...    maximum=1
    ...    type=myCustomAlarmType
    Should Be Equal    ${alarms[0]["someOtherCustomFragment"]["nested"]["value"]}    extra info
    Log    ${alarms}

Child devices support sending alarms using text fragment
    Execute Command
    ...    tedge mqtt pub te/device/${CHILD_SN}///a/childAlarmType1 '{ "severity": "critical", "text": "Some test alarm" }'
    ${alarms}=    Device Should Have Alarm/s
    ...    expected_text=Some test alarm
    ...    severity=CRITICAL
    ...    minimum=1
    ...    maximum=1
    ...    type=childAlarmType1
    Log    ${alarms}

Child devices support sending inventory data via c8y topic
    Execute Command    tedge mqtt pub "c8y/inventory/managedObjects/update/${CHILD_SN}" '{"custom":{"fragment":"yes"}}'
    ${mo}=    Device Should Have Fragments    custom
    Should Be Equal    ${mo["custom"]["fragment"]}    yes

Child device supports sending custom child device measurements directly to c8y
    Execute Command
    ...    tedge mqtt pub "c8y/measurement/measurements/create" '{"time":"2023-03-20T08:03:56.940907Z","externalSource":{"externalId":"${CHILD_SN}","type":"c8y_Serial"},"environment":{"temperature":{"value":29.9,"unit":"°C"}},"type":"10min_average","meta":{"sensorLocation":"Brisbane, Australia"}}'
    Cumulocity.Set Device    ${CHILD_SN}
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
    ...    tedge mqtt pub "c8y/measurement/measurements/createBulk" '{"measurements":[{"time":"2024-12-01T02:00:00Z","externalSource":{"externalId":"${CHILD_SN}","type":"c8y_Serial"},"outside":{"temperature":{"value":2.5,"unit":"°C"}},"type":"1min_average"},{"time":"2024-12-01T02:01:00Z","externalSource":{"externalId":"${CHILD_SN}","type":"c8y_Serial"},"outside":{"temperature":{"value":3.5,"unit":"°C"}},"type":"1min_average"}]}'
    Cumulocity.Set Device    ${CHILD_SN}
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
    ...    tedge mqtt pub "c8y/event/events/createBulk" '{"events":[{"time":"2024-12-01T02:00:00Z","externalSource":{"externalId":"${CHILD_SN}","type":"c8y_Serial"},"text":"event 1","type":"bulkevent"},{"time":"2024-12-01T02:01:00Z","externalSource":{"externalId":"${CHILD_SN}","type":"c8y_Serial"},"text":"event 2","type":"bulkevent"}]}'
    Cumulocity.Set Device    ${CHILD_SN}
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
    ...    tedge mqtt pub "c8y/alarm/alarms/createBulk" '{"alarms":[{"time":"2024-12-01T02:00:00Z","externalSource":{"externalId":"${CHILD_SN}","type":"c8y_Serial"},"text":"alarm 1","severity":"MAJOR","type":"bulkalarm1"},{"time":"2024-12-01T02:01:00Z","externalSource":{"externalId":"${CHILD_SN}","type":"c8y_Serial"},"text":"alarm 2","severity":"MINOR","type":"bulkalarm2"}]}'
    Cumulocity.Set Device    ${CHILD_SN}
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

#
# Services
#
# measurements

Send measurements to an unregistered child service
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}/service/app1/m/m_type '{"temperature": 30.1}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    min_count=1    max_count=1    name=app1

    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app1
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=m_type
    Should Be Equal    ${measurements[0]["type"]}    m_type
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    30.1

Send measurements to a registered child service
    Execute Command
    ...    tedge mqtt pub --retain te/device/${CHILD_SN}/service/app2 '{"@type":"service","@parent":"device/${CHILD_SN}//"}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    name=app2    min_count=1    max_count=1

    Execute Command    tedge mqtt pub te/device/${CHILD_SN}/service/app2/m/m_type '{"temperature": 30.1}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app2
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=m_type
    Should Be Equal    ${measurements[0]["type"]}    m_type
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    30.1

# alarms

Send alarms to an unregistered child service
    Execute Command
    ...    tedge mqtt pub te/device/${CHILD_SN}/service/app3/a/alarm_001 '{"text": "test alarm","severity":"major"}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    min_count=1    max_count=1    name=app3

    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app3
    ${alarms}=    Device Should Have Alarm/s    expected_text=test alarm    type=alarm_001    minimum=1    maximum=1
    Should Be Equal    ${alarms[0]["type"]}    alarm_001
    Should Be Equal    ${alarms[0]["severity"]}    MAJOR

Send alarms to a registered child service
    Execute Command
    ...    tedge mqtt pub --retain te/device/${CHILD_SN}/service/app4 '{"@type":"service","@parent":"device/${CHILD_SN}//"}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    name=app4    min_count=1    max_count=1

    Execute Command    tedge mqtt pub te/device/${CHILD_SN}/service/app4/a/alarm_002 '{"text": "test alarm"}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app4
    ${alarms}=    Device Should Have Alarm/s    expected_text=test alarm    type=alarm_002    minimum=1    maximum=1
    Should Be Equal    ${alarms[0]["type"]}    alarm_002

# events

Send events to an unregistered child service
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}/service/app5/e/event_001 '{"text": "test event"}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    name=app5    min_count=1    max_count=1

    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app5
    Device Should Have Event/s    expected_text=test event    type=event_001    minimum=1    maximum=1

Send events to a registered child service
    Execute Command
    ...    tedge mqtt pub --retain te/device/${CHILD_SN}/service/app6 '{"@type":"service","@parent":"device/${CHILD_SN}//"}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    name=app6    min_count=1    max_count=1
    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app6
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}/service/app6/e/event_002 '{"text": "test event"}'
    Device Should Have Event/s    expected_text=test event    type=event_002    minimum=1    maximum=1

# Nested child devices

Nested child devices support sending measurement
    ${nested_child}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command    tedge mqtt pub te/device/${nested_child}///m/ '{ "temperature": 25 }'
    Cumulocity.Device Should Exist    ${nested_child}
    ${measurements}=    Device Should Have Measurements
    ...    type=ThinEdgeMeasurement
    ...    value=temperature
    ...    series=temperature
    ...    minimum=1
    ...    maximum=1
    Log    ${measurements}

Nested child devices support sending alarm
    ${nested_child}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command
    ...    tedge mqtt pub te/device/${nested_child}///a/test_alarm '{ "severity":"critical","text":"temperature alarm" }'
    Cumulocity.Device Should Exist    ${nested_child}
    ${alarm}=    Device Should Have Alarm/s
    ...    type=test_alarm
    ...    expected_text=temperature alarm
    ...    severity=CRITICAL
    ...    minimum=1
    ...    maximum=1
    Log    ${alarm}

Nested child devices support sending event
    ${nested_child}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command    tedge mqtt pub te/device/${nested_child}///e/event_nested '{ "text":"nested child event" }'
    Cumulocity.Device Should Exist    ${nested_child}
    Device Should Have Event/s    expected_text=nested child event    type=event_nested    minimum=1    maximum=1

# Nested child device services

Nested child device service support sending simple measurements
    ${nested_child}=    Get Random Name
    ${service_name}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}/service/${service_name}' '{"@type":"service","@parent":"device/${nested_child}//","@id":"${service_name}"}'
    Execute Command
    ...    tedge mqtt pub te/device/${nested_child}/service/${service_name}/m/m_type '{ "temperature": 30.1 }'
    Cumulocity.Device Should Exist    ${nested_child}
    Cumulocity.Should Have Services    name=${service_name}    min_count=1    max_count=1
    Cumulocity.Device Should Exist    ${service_name}
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1
    Should Be Equal    ${measurements[0]["type"]}    m_type
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    30.1
    Log    ${measurements}

Nested child device service support sending events
    ${nested_child}=    Get Random Name
    ${service_name}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}/service/${service_name}' '{"@type":"service","@parent":"device/${nested_child}//","@id":"${service_name}"}'
    Execute Command
    ...    tedge mqtt pub te/device/${nested_child}/service/${service_name}/e/e_type '{ "text": "nested device service started" }'
    Cumulocity.Device Should Exist    ${nested_child}
    Cumulocity.Should Have Services    name=${service_name}    min_count=1    max_count=1
    Cumulocity.Device Should Exist    ${service_name}
    Device Should Have Event/s    expected_text=nested device service started    type=e_type    minimum=1    maximum=1

Nested child device service support sending alarm
    ${nested_child}=    Get Random Name
    ${service_name}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}/service/${service_name}' '{"@type":"service","@parent":"device/${nested_child}//","@id":"${service_name}"}'
    Execute Command
    ...    tedge mqtt pub te/device/${nested_child}/service/${service_name}/a/test_alarm '{ "severity":"critical","text":"temperature alarm" }'
    Cumulocity.Device Should Exist    ${nested_child}
    Cumulocity.Should Have Services    name=${service_name}    min_count=1    max_count=1
    Cumulocity.Device Should Exist    ${service_name}
    ${alarm}=    Device Should Have Alarm/s
    ...    type=test_alarm
    ...    expected_text=temperature alarm
    ...    severity=CRITICAL
    ...    minimum=1
    ...    maximum=1
    Log    ${alarm}

Child device registered even on bad input
    ${nested_child}=    Get Random Name
    Execute Command    tedge mqtt pub te/device/${nested_child}///e/event_001 'bad json'
    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${nested_child}

    Execute Command    tedge mqtt pub te/device/${nested_child}///e/event_001 '{"text": "test event"}'
    Device Should Have Event/s    expected_text=test event    type=event_001    minimum=1    maximum=1


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}_child1
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_SN}"}'
    Device Should Exist    ${DEVICE_SN}
    Device Should Exist    ${CHILD_SN}

    Service Health Status Should Be Up    tedge-mapper-c8y
