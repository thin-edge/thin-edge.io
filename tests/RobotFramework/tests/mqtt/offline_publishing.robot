*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Setup          Setup Test
Test Teardown       Get Logs


*** Test Cases ***
Publish event whilst mapper is down
    [Documentation]    The mapper should publish the event to the cloud when it comes back online
    ${event_type}=    Get Random Name
    Stop Service    tedge-mapper-c8y
    Execute Command    tedge mqtt pub -q 1 te/device/main///e/${event_type} '{"text":"test message"}'
    Start Service    tedge-mapper-c8y
    Cumulocity.Device Should Have Event/s    type=${event_type}    expected_text=test message


*** Keywords ***
Setup Test
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
