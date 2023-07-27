*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:telemetry
Suite Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Child devices support sending simple measurements
    Execute Command    tedge mqtt pub te/device/${CHILD_NAME}///m/ '{ "temperature": 25 }'
    Device Should Exist                      ${CHILD_SN}
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=temperature    series=temperature
    Log    ${measurements}


Child devices support sending custom measurements
    Execute Command    tedge mqtt pub te/device/${CHILD_NAME}///m/ '{ "current": {"L1": 9.5, "L2": 1.3} }'
    Device Should Exist                      ${CHILD_SN}
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1    type=ThinEdgeMeasurement    value=current    series=L1
    Log    ${measurements}


Child devices support sending custom events
    Execute Command    tedge mqtt pub te/device/${CHILD_NAME}///e/myCustomType1 '{ "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    Device Should Exist                      ${CHILD_SN}
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=myCustomType1    fragment=someOtherCustomFragment
    Log    ${events}


Child devices support sending custom events overriding the type
    Execute Command    tedge mqtt pub te/device/${CHILD_NAME}///e/myCustomType '{"type": "otherType", "text": "Some test event", "someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    Device Should Exist                      ${CHILD_SN}
    ${events}=    Device Should Have Event/s    expected_text=Some test event    with_attachment=False    minimum=1    maximum=1    type=otherType    fragment=someOtherCustomFragment
    Log    ${events}


Child devices support sending custom alarms #1699
    [Tags]    \#1699
    Execute Command    tedge mqtt pub te/device/${CHILD_NAME}///a/myCustomAlarmType '{ "text": "Some test alarm", "severity":"critical","someOtherCustomFragment": {"nested":{"value": "extra info"}} }'
    Device Should Exist                      ${CHILD_SN}
    ${alarms}=    Device Should Have Alarm/s    expected_text=Some test alarm    severity=CRITICAL    minimum=1    maximum=1    type=myCustomAlarmType
    Should Be Equal    ${alarms[0]["someOtherCustomFragment"]["nested"]["value"]}    extra info
    Log    ${alarms}

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Set Suite Variable    $CHILD_SN   ${DEVICE_SN}:device:${DEVICE_SN}_child1
    Set Suite Variable    $CHILD_NAME   ${DEVICE_SN}_child1 
    Device Should Exist                      ${DEVICE_SN}
