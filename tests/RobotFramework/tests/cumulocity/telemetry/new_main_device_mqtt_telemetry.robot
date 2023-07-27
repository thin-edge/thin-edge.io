*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:telemetry
Suite Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Thin-edge devices support sending simple measurements
    Execute Command    tedge mqtt pub te/device/main///m/ '{ "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=temperature    series=temperature
    Log    ${measurements}


Thin-edge devices support sending simple measurements with custom type
    Execute Command    tedge mqtt pub te/device/main///m/ '{ "type":"CustomType", "temperature": 25 }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=CustomType    value=temperature    series=temperature
    Log    ${measurements}    

Thin-edge devices support sending custom measurements
    Execute Command    tedge mqtt pub te/device/main///m/ '{ "current": {"L1": 9.5, "L2": 1.3} }'
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=current    series=L1
    Log    ${measurements}


Thin-edge devices support sending custom events
    Execute Command    tedge mqtt pub te/device/main///e/myCustomType1 '{ "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=myCustomType1    fragment=someOtherCustomFragment
    Log    ${events}


Thin-edge devices support sending custom events overriding the type
    Execute Command    tedge mqtt pub te/device/main///e/myCustomType '{"type": "otherType", "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=otherType    fragment=someOtherCustomFragment
    Log    ${events}


Thin-edge devices support sending custom alarms #1699
    [Tags]    \#1699
    Execute Command    tedge mqtt pub te/device/main///a/myCustomAlarmType '{ "text": "Some test alarm", "severity":"critical", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=CRITICAL    minimum=1    maximum=1    type=myCustomAlarmType
    Should Be Equal    ${alarms[0]["someOtherCustomFragment"]["nested"]["value"]}    extra info
    Log    ${alarms}



*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN  
    Device Should Exist                      ${DEVICE_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y
