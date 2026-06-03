*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Teardown      Custom Teardown
Test Setup          Custom Setup

Test Tags           theme:configuration


*** Test Cases ***
Use placeholders in tedge.toml
    [Documentation]    Check that `{}` placeholders in `/etc/tedge/system.toml`
    ...    are properly replaced by service names
    ...    when starting or restarting services using tedge cli
    ...    even when the placeholders are not defined as standalone arguments
    Transfer To Device    ${CURDIR}/resources/custom_system.toml    /etc/tedge/system.toml

    Execute Command    tedge reconnect c8y
    Service Should Be Running    tedge-mapper-c8y


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}
    Service Should Be Running    tedge-mapper-c8y

Custom Teardown
    Get Suite Logs
