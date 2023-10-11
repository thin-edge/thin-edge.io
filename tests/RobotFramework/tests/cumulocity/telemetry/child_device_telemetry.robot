*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:telemetry
Suite Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Child devices support sending simple measurements
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///m/ '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=temperature    series=temperature
    Log    ${measurements}

Child devices support sending simple measurements with custom type in topic
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///m/CustomType_topic '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=CustomType_topic    value=temperature    series=temperature
    Log    ${measurements}


Child devices support sending simple measurements with custom type in payload
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///m/CustomType_topic '{ "type":"CustomType_payload","temperature": 25 }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=CustomType_payload    value=temperature    series=temperature
    Log    ${measurements}

Child devices support sending custom measurements
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///m/ '{ "current": {"L1": 9.5, "L2": 1.3} }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=current    series=L1
    Log    ${measurements}


Child devices support sending custom events
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///e/myCustomType1 '{ "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=myCustomType1    fragment=someOtherCustomFragment
    Log    ${events}


Child devices support sending custom events overriding the type
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///e/myCustomType '{"type": "otherType", "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=otherType    fragment=someOtherCustomFragment
    Log    ${events}


 Child devices support sending custom events without type in topic
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///e/ '{"text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=ThinEdgeEvent    fragment=someOtherCustomFragment
    Log    ${events}


Child devices support sending custom alarms #1699
    [Tags]    \#1699
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///a/myCustomAlarmType '{ "severity": "critical", "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=CRITICAL    minimum=1    maximum=1    type=myCustomAlarmType
    Should Be Equal    ${alarms[0]["someOtherCustomFragment"]["nested"]["value"]}    extra info
    Log    ${alarms}


Child devices support sending alarms using text fragment
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}///a/childAlarmType1 '{ "severity": "critical", "text": "Some test alarm" }'
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=CRITICAL    minimum=1    maximum=1    type=childAlarmType1
    Log    ${alarms}


Child devices support sending inventory data via c8y topic
    Execute Command    tedge mqtt pub "c8y/inventory/managedObjects/update/${CHILD_SN}" '{"custom":{"fragment":"yes"}}'
    ${mo}=    Device Should Have Fragments    custom
    Should Be Equal    ${mo["custom"]["fragment"]}    yes


Child devices support sending inventory data via tedge topic with type
    Execute Command    tedge mqtt pub "te/device/${CHILD_SN}///twin/device_OS" '{"family":"Debian","version":11,"complex":[1,"2",3],"object":{"foo":"bar"}}'
    Cumulocity.Set Device    ${CHILD_SN}
    ${mo}=    Device Should Have Fragments    device_OS
    Should Be Equal    ${mo["device_OS"]["family"]}    Debian
    Should Be Equal As Integers    ${mo["device_OS"]["version"]}    11

    Should Be Equal As Integers    ${mo["device_OS"]["complex"][0]}    1
    Should Be Equal As Strings    ${mo["device_OS"]["complex"][1]}    2
    Should Be Equal As Integers    ${mo["device_OS"]["complex"][2]}    3
    Should Be Equal    ${mo["device_OS"]["object"]["foo"]}    bar


Child devices supports sending inventory data via tedge topic to root fragments
    Execute Command    tedge mqtt pub "te/device/${CHILD_SN}///twin/subtype" '"LinuxDeviceA"'
    Execute Command    tedge mqtt pub "te/device/${CHILD_SN}///twin/type" '"ShouldBeIgnored"'
    Execute Command    tedge mqtt pub "te/device/${CHILD_SN}///twin/name" '"ShouldBeIgnored"'
    Cumulocity.Set Device    ${CHILD_SN}
    ${mo}=    Device Should Have Fragments    subtype
    Should Be Equal    ${mo["subtype"]}    LinuxDeviceA
    Should Be Equal    ${mo["type"]}    thin-edge.io-child
    Should Be Equal    ${mo["name"]}    ${CHILD_SN}


Child device supports sending custom child device measurements directly to c8y
    Execute Command    tedge mqtt pub "c8y/measurement/measurements/create" '{"time":"2023-03-20T08:03:56.940907Z","externalSource":{"externalId":"${CHILD_SN}","type":"c8y_Serial"},"environment":{"temperature":{"value":29.9,"unit":"°C"}},"type":"10min_average","meta":{"sensorLocation":"Brisbane, Australia"}}'
    Cumulocity.Set Device    ${CHILD_SN}
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    value=environment    series=temperature    type=10min_average
    Should Be Equal As Numbers    ${measurements[0]["environment"]["temperature"]["value"]}    29.9
    Should Be Equal    ${measurements[0]["meta"]["sensorLocation"]}    Brisbane, Australia
    Should Be Equal    ${measurements[0]["type"]}    10min_average

Nested child devices support sending inventory data via tedge topic
    ${nested_child}=    Get Random Name
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'

    Execute Command    tedge mqtt pub "te/device/${nested_child}///twin/device_OS" '{"family":"Debian","version":11}'
    Execute Command    tedge mqtt pub "te/device/${nested_child}///twin/subtype" '"LinuxDeviceB"'
    Execute Command    tedge mqtt pub "te/device/${nested_child}///twin/type" '"ShouldBeIgnored"'
    Execute Command    tedge mqtt pub "te/device/${nested_child}///twin/name" '"ShouldBeIgnored"'

    Cumulocity.Set Device    ${nested_child}
    ${mo}=    Device Should Have Fragments    device_OS
    Should Be Equal    ${mo["device_OS"]["family"]}    Debian
    Should Be Equal As Integers    ${mo["device_OS"]["version"]}    11
    ${mo}=    Device Should Have Fragments    subtype
    Should Be Equal    ${mo["subtype"]}    LinuxDeviceB
    Should Be Equal    ${mo["type"]}    thin-edge.io-child
    Should Be Equal    ${mo["name"]}    ${nested_child}


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
    Should Be Equal    ${measurements[0]["type"]}   m_type
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    30.1

Send measurements to a registered child service
    Execute Command    tedge mqtt pub --retain te/device/${CHILD_SN}/service/app2 '{"@type":"service","@parent":"device/${CHILD_SN}//"}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    name=app2    min_count=1    max_count=1
    
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}/service/app2/m/m_type '{"temperature": 30.1}'
    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app2
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=m_type
    Should Be Equal    ${measurements[0]["type"]}    m_type
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    30.1    

# alarms
Send alarms to an unregistered child service
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}/service/app3/a/alarm_001 '{"text": "test alarm","severity":"major"}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    min_count=1    max_count=1    name=app3

    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app3
    ${alarms}=    Device Should Have Alarm/s    expected_text=test alarm    type=alarm_001    minimum=1    maximum=1
    Should Be Equal    ${alarms[0]["type"]}    alarm_001
    Should Be Equal    ${alarms[0]["severity"]}    MAJOR

Send alarms to a registered child service
    Execute Command    tedge mqtt pub --retain te/device/${CHILD_SN}/service/app4 '{"@type":"service","@parent":"device/${CHILD_SN}//"}'
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
    Execute Command    tedge mqtt pub --retain te/device/${CHILD_SN}/service/app6 '{"@type":"service","@parent":"device/${CHILD_SN}//"}'
    Cumulocity.Device Should Exist    ${CHILD_SN}
    Cumulocity.Should Have Services    name=app6    min_count=1    max_count=1
    Cumulocity.Device Should Exist    ${DEVICE_SN}:device:${CHILD_SN}:service:app6
    Execute Command    tedge mqtt pub te/device/${CHILD_SN}/service/app6/e/event_002 '{"text": "test event"}'
    Device Should Have Event/s    expected_text=test event    type=event_002    minimum=1    maximum=1
  
# Nested child devices
Nested child devices support sending measurement
    ${nested_child}=    Get Random Name
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command    tedge mqtt pub te/device/${nested_child}///m/ '{ "temperature": 25 }'
    Cumulocity.Device Should Exist    ${nested_child}
    ${measurements}=    Device Should Have Measurements     type=ThinEdgeMeasurement    value=temperature    series=temperature       minimum=1    maximum=1
    Log    ${measurements}


Nested child devices support sending alarm
    ${nested_child}=    Get Random Name
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command    tedge mqtt pub te/device/${nested_child}///a/test_alarm '{ "severity":"critical","text":"temperature alarm" }'
    Cumulocity.Device Should Exist    ${nested_child}
    ${alarm}=    Device Should Have Alarm/s    type=test_alarm    expected_text=temperature alarm   severity=CRITICAL    minimum=1    maximum=1  
    Log    ${alarm}

Nested child devices support sending event
    ${nested_child}=    Get Random Name
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command    tedge mqtt pub te/device/${nested_child}///e/event_nested '{ "text":"nested child event" }'
    Cumulocity.Device Should Exist    ${nested_child}
    Device Should Have Event/s    expected_text=nested child event    type=event_nested    minimum=1    maximum=1
 
# Nested child device services 
Nested child device service support sending simple measurements
    ${nested_child}=    Get Random Name    
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}/service/nested_ms_service' '{"@type":"service","@parent":"device/${nested_child}//","@id":"nested_ms_service"}'
    Execute Command    tedge mqtt pub te/device/${nested_child}/service/nested_ms_service/m/m_type '{ "temperature": 30.1 }'
    Cumulocity.Device Should Exist    ${nested_child}
    Cumulocity.Should Have Services    name=nested_ms_service    min_count=1    max_count=1  
    Cumulocity.Device Should Exist   nested_ms_service
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1
    Should Be Equal    ${measurements[0]["type"]}    m_type
    Should Be Equal As Numbers    ${measurements[0]["temperature"]["temperature"]["value"]}    30.1
    Log    ${measurements}

Nested child device service support sending events
    ${nested_child}=    Get Random Name    
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}/service/nested_event_service' '{"@type":"service","@parent":"device/${nested_child}//","@id":"nested_event_service"}'
    Execute Command    tedge mqtt pub te/device/${nested_child}/service/nested_event_service/e/e_type '{ "text": "nested device service started" }'
    Cumulocity.Device Should Exist    ${nested_child}    
    Cumulocity.Should Have Services    name=nested_event_service    min_count=1    max_count=1  
    Cumulocity.Device Should Exist   nested_event_service
    Device Should Have Event/s    expected_text=nested device service started    type=e_type    minimum=1    maximum=1
   
Nested child device service support sending alarm
    ${nested_child}=    Get Random Name
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${CHILD_SN}//","@id":"${nested_child}"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${nested_child}/service/nested_alarm_service' '{"@type":"service","@parent":"device/${nested_child}//","@id":"nested_alarm_service"}'
    Execute Command    tedge mqtt pub te/device/${nested_child}/service/nested_alarm_service/a/test_alarm '{ "severity":"critical","text":"temperature alarm" }'
    Cumulocity.Device Should Exist    ${nested_child}    
    Cumulocity.Should Have Services    name=nested_alarm_service    min_count=1    max_count=1  
    Cumulocity.Device Should Exist   nested_alarm_service
    ${alarm}=    Device Should Have Alarm/s    type=test_alarm    expected_text=temperature alarm   severity=CRITICAL    minimum=1    maximum=1  
    Log    ${alarm}

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}_child1
    Execute Command    mkdir -p /etc/tedge/operations/c8y/${CHILD_SN}
    Restart Service    tedge-mapper-c8y
    Device Should Exist                      ${DEVICE_SN}
    Device Should Exist                      ${CHILD_SN}

    Service Health Status Should Be Up    tedge-mapper-c8y
