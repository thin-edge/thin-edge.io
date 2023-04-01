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
    [Arguments]    ${folder}
    ${timestamp}    ThinEdgeIO.Get Test Start Time
    #Raising alarms + adding custom fragment
    Stop Service    tedge-mapper-c8y
    Execute Command    sudo tedge mqtt pub 'tedge/alarms/${folder}/temperature_${folder}' '{ "text": "Temperature is ${folder}", "details": "A custom alarm info ${folder}" }' -q 2 -r
    Device Should Not Have Alarm/s    type=temperature_${folder}
    Start Service    tedge-mapper-c8y
    Device Should Have Alarm/s    minimum=0    maximum=1    expected_text=Temperature is ${folder}    type=temperature_${folder}    severity=${folder}
    Device Should Have Alarm/s    minimum=0    maximum=1    expected_text=A custom alarm info ${folder}    type=temperature_${folder}    severity=${folder}     
    #Clearing alarms
    Execute Command    sudo tedge mqtt pub tedge/alarms/${folder}/temperature_${folder} "" -q 2 -r
    Device Should Not Have Alarm/s    type=temperature_${folder}

