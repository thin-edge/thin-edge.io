*** Settings ***
Resource        ../../../resources/common.resource
Library         ThinEdgeIO
Library         Cumulocity

Test Setup     Custom Setup
Test Teardown    Get Logs

Force Tags      theme:telemetry    theme:c8y    theme:alarms


*** Test Cases ***
Check retained alarms
    [Template]    Send retained Alarm
    critical
    major
    minor
    warning

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}

Send retained Alarm
    [Documentation]    https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/tutorials/raise-alarm.md
    [Arguments]    ${severity}
    ${timestamp}    ThinEdgeIO.Get Test Start Time
    #Raising alarms + adding custom fragment
    Stop Service    tedge-mapper-c8y
    Execute Command    sudo tedge mqtt pub 'tedge/alarms/${severity}/temperature_${severity}' '{ "text": "Temperature is ${severity}", "details": "A custom alarm info ${severity}" }' -q 2 -r
    Device Should Not Have Alarm/s    type=temperature_${severity}
    Start Service    tedge-mapper-c8y
    Device Should Have Alarm/s    minimum=0    maximum=1    expected_text=Temperature is ${severity}    type=temperature_${severity}    severity=${severity}
    Device Should Have Alarm/s    minimum=0    maximum=1    expected_text=A custom alarm info ${severity}    type=temperature_${severity}    severity=${severity}     
    #Clearing alarms
    Execute Command    sudo tedge mqtt pub tedge/alarms/${severity}/temperature_${severity} "" -q 2 -r
    Device Should Not Have Alarm/s    type=temperature_${severity}
