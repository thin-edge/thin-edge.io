*** Settings ***
Resource    ../../../resources/common.resource
Library    ThinEdgeIO

Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Connection test
    [Documentation]    Repeatedly test the cloud connection
    FOR    ${attempt}    IN RANGE    0    10    1
        ${output}=    Execute Command    tedge connect c8y --test    timeout=10
        Should Not Contain  ${output}    connection check failed
    END


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge reconnect c8y
