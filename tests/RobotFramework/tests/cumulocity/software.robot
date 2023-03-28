*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:software    theme:plugins
Test Setup       Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Software list should be populated during startup
    Device Should Have Installed Software    tedge    timeout=120

Install software via Cumulocity
    ${OPERATION}=    Install Software        c8y-remote-access-plugin    # TODO: Use different package
    Operation Should Be SUCCESSFUL           ${OPERATION}    timeout=60
    Device Should Have Installed Software    c8y-remote-access-plugin

Software list should only show currently installed software and not candidates
    ${EXPECTED_VERSION}=    Execute Command    dpkg -s tedge | grep "^Version: " | cut -d' ' -f2    strip=True
    Device Should Have Installed Software    tedge,^${EXPECTED_VERSION}::apt$        timeout=120

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    Set Test Variable    $DEVICE_SN
    Should Have MQTT Messages    tedge/health/tedge-mapper-c8y
    [Documentation]    WORKAROUND: #1731 The tedge-mapper-c8y is restarted due to a suspected race condition between the mapper and tedge-agent which results in the software list message being lost
    ${timestamp}=        Get Unix Timestamp
    Restart Service    tedge-mapper-c8y
    Should Have MQTT Messages    tedge/health/tedge-mapper-c8y    date_from=${timestamp}
