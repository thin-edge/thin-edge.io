*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_agent


*** Test Cases ***
Device profile is included in supported operations
    Should Have MQTT Messages    te/device/main///cmd/device_profile    message_pattern=^{}$
    Should Contain Supported Operations    c8y_DeviceProfile


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
