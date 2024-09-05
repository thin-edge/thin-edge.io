*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_agent


*** Test Cases ***
On start process all the pending operations
    [Tags]    \#2369
    Stop Service    tedge-agent
    FOR    ${id}    IN RANGE    0    10
        Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/test-${id} '{"status":"init"}'
    END
    Start Service    tedge-agent
    Sleep    5s
    FOR    ${id}    IN RANGE    0    10
        Should Have MQTT Messages
        ...    te/device/main///cmd/software_list/test-${id}
        ...    message_pattern=.*successful.*
        ...    minimum=1
        Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/test-${id} ''
    END


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=False
    Set Suite Variable    $DEVICE_SN
