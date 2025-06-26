*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

# Test Setup    Custom Setup
Test Teardown       Get Logs

Test Tags           theme:configuration    known-issue


*** Test Cases ***
Startup with invalid system.toml
    Skip    msg=Known issue. tedge does not startup if the system.toml file contains invalid toml content
    Setup    connect=${False}
    ${DEVICE_SN}=    Execute Command    tedge config get device.id
    Execute Command    cmd=echo -n "[new-section]\ntest = invalid" > /etc/tedge/system.toml
    Execute Command    cmd=tedge connect c8y
    Device Should Exist    ${DEVICE_SN}

Startup with invalid tedge.toml
    Skip    msg=Known issue. tedge does not startup if the tedge.toml file contains invalid toml content
    Setup    connect=${False}
    ${DEVICE_SN}=    Execute Command    tedge config get device.id
    Execute Command    cmd=echo -n "[new-section]\ntest = invalid" > /etc/tedge/tedge.toml
    Execute Command    cmd=tedge connect c8y
    Device Should Exist    ${DEVICE_SN}
