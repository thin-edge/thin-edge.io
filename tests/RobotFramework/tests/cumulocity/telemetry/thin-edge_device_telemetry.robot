*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:telemetry
Suite Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Thin-edge devices support sending simple measurements
    Execute Command    tedge mqtt pub tedge/measurements '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=temperature    series=temperature
    Log    ${measurements}


Thin-edge devices support sending simple measurements with custom type
    Execute Command    tedge mqtt pub tedge/measurements '{ "type":"CustomType", "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=CustomType    value=temperature    series=temperature
    Log    ${measurements}    


Thin-edge devices support sending custom measurements
    Execute Command    tedge mqtt pub tedge/measurements '{ "current": {"L1": 9.5, "L2": 1.3} }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=current    series=L1
    Log    ${measurements}


Thin-edge devices support sending custom events
    Execute Command    tedge mqtt pub tedge/events/myCustomType1 '{ "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=myCustomType1    fragment=someOtherCustomFragment
    Log    ${events}


Thin-edge devices support sending custom events overriding the type
    Execute Command    tedge mqtt pub tedge/events/myCustomType '{"type": "otherType", "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=otherType    fragment=someOtherCustomFragment
    Log    ${events}


Thin-edge devices support sending custom alarms #1699
    [Tags]    \#1699
    Execute Command    tedge mqtt pub tedge/alarms/critical/myCustomAlarmType '{ "text": "Some test alarm", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=CRITICAL    minimum=1    maximum=1    type=myCustomAlarmType
    Should Be Equal    ${alarms[0]["someOtherCustomFragment"]["nested"]["value"]}    extra info
    Log    ${alarms}


Thin-edge device supports sending custom Thin-edge device measurements directly to c8y
    Execute Command    tedge mqtt pub "c8y/measurement/measurements/create" '{"time":"2023-03-20T08:03:56.940907Z","environment":{"temperature":{"value":29.9,"unit":"°C"}},"type":"10min_average","meta":{"sensorLocation":"Brisbane, Australia"}}'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    value=environment    series=temperature    type=10min_average
    Should Be Equal As Numbers    ${measurements[0]["environment"]["temperature"]["value"]}    29.9
    Should Be Equal    ${measurements[0]["meta"]["sensorLocation"]}    Brisbane, Australia
    Should Be Equal    ${measurements[0]["type"]}    10min_average


Thin-edge device support sending inventory data via c8y topic
    Execute Command    tedge mqtt pub "c8y/inventory/managedObjects/update/${DEVICE_SN}" '{"parentInfo":{"nested":{"name":"complex"}},"subType":"customType"}'
    Cumulocity.Set Device    ${DEVICE_SN}
    ${mo}=    Device Should Have Fragments    parentInfo    subType
    Should Be Equal    ${mo["parentInfo"]["nested"]["name"]}    complex
    Should Be Equal    ${mo["subType"]}    customType


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN 
    Device Should Exist                      ${DEVICE_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y
