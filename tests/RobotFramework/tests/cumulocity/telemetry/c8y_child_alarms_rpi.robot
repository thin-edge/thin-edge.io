*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:telemetry    theme:childdevices


*** Variables ***
${DEVICE_SN}    ${EMPTY}
${CHILD_SN}     ${EMPTY}
${CHILD_XID}    ${EMPTY}


*** Test Cases ***
Define Child device 1 ID
    Set Suite Variable    $CHILD_SN    child01
    Set Suite Variable    $CHILD_XID    ${DEVICE_SN}:device:${CHILD_SN}

Normal case when the child device does not exist on c8y cloud
    # Sending child alarm
    Execute Command
    ...    sudo tedge mqtt pub 'te/device/${CHILD_SN}///a/temperature_high' '{ "severity": "critical", "text": "Temperature is very high", "time": "2021-01-01T05:30:45+00:00" }' -q 2 -r
    # Check Child device creation
    Set Device    ${DEVICE_SN}
    Should Be A Child Device Of Device    ${CHILD_XID}

    # Check created alarm
    Set Device    ${CHILD_XID}
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    # Should be the only alarm there
    ${alarms}=    Device Should Have Alarm/s
    ...    minimum=1
    ...    maximum=1
    ...    expected_text=Temperature is very high
    ...    type=temperature_high
    ...    severity=CRITICAL

Normal case when the child device already exists
    # Sending child alarm again
    Execute Command
    ...    sudo tedge mqtt pub 'te/device/${CHILD_SN}///a/temperature_high' '{ "severity": "critical", "text": "Temperature is very high", "time": "2021-01-02T05:30:45+00:00" }' -q 2 -r

    # Check created second alarm
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    updated_after=2021-01-02
    ${alarms}=    Device Should Have Alarm/s
    ...    minimum=1
    ...    maximum=1
    ...    expected_text=Temperature is very high
    ...    type=temperature_high
    ...    severity=CRITICAL
    ...    updated_after=2021-01-02

Reconciliation when the new alarm message arrives, restart the mapper
    Execute Command    sudo systemctl stop tedge-mapper-c8y.service
    Execute Command
    ...    sudo tedge mqtt pub 'te/device/${CHILD_SN}///a/temperature_high' '{ "severity": "critical", "text": "Temperature is very high", "time": "2021-01-03T05:30:45+00:00" }' -q 2 -r
    Execute Command    sudo systemctl start tedge-mapper-c8y.service

    # Check created second alarm
    ${alarms}=    Device Should Have Alarm/s    minimum=1    maximum=1    updated_after=2021-01-03
    ${alarms}=    Device Should Have Alarm/s
    ...    minimum=1
    ...    maximum=1
    ...    expected_text=Temperature is very high
    ...    type=temperature_high
    ...    severity=CRITICAL
    ...    updated_after=2021-01-03

Reconciliation when the alarm that is cleared
    Execute Command    sudo systemctl stop tedge-mapper-c8y.service
    Execute Command    sudo tedge mqtt pub 'te/device/${CHILD_SN}///a/temperature_high' '' -q 2 -r
    Execute Command    sudo systemctl start tedge-mapper-c8y.service
    Device Should Not Have Alarm/s


*** Keywords ***
Custom Setup
    ${device_sn}=    Setup
    Set Suite Variable    $DEVICE_SN    ${device_sn}
