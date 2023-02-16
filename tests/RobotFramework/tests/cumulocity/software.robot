*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:software    theme:plugins
Test Teardown    Get Logs

*** Test Cases ***
Software list should be populated during startup
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    Device Should Have Installed Software    tedge

Install software via Cumulocity
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    Sleep    12s    reason=FIX: Wait for device to be ready for operations
    ${OPERATION}=    Install Software        c8y-remote-access-plugin    # TODO: Use different package
    Operation Should Be SUCCESSFUL           ${OPERATION}    timeout=60
    Device Should Have Installed Software    c8y-remote-access-plugin

Software list should only show currently installed software and not candidates
    [Tags]    flakey
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    ${EXPECTED_VERSION}=    Execute Command    dpkg -s tedge | grep "^Version: " | cut -d' ' -f2    strip=True
    Device Should Have Installed Software    tedge,^${EXPECTED_VERSION}::apt$
