*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:software    theme:plugins
Test Setup       Custom Setup
Test Teardown    Custom Teardown

*** Test Cases ***
Software list should be populated during startup
    Device Should Have Installed Software    tedge    timeout=120

Install software via Cumulocity
    ${OPERATION}=    Install Software        c8y-remote-access-plugin    # TODO: Use different package
    Operation Should Be SUCCESSFUL           ${OPERATION}    timeout=60
    Device Should Have Installed Software    c8y-remote-access-plugin

tedge-agent should terminate on SIGINT while downloading file
    [Documentation]    The test uses a custom local http server with throttling applied to it to ensure
    ...                the download does not complete before stopping the tedge-agent
    ${start_time}=    Get Unix Timestamp
    ${OPERATION}=    Install Software        test-very-large-software,1.0,http://localhost/speedlimit/10MB

    # wait for the download to start by waiting for a specific marker to appear in the logs
    Logs Should Contain    text=download::download: Downloading file from url    date_from=${start_time}
    Operation Should Not Be PENDING          ${OPERATION}

    # Service should stop within 5s
    Stop tedge-agent

Software list should only show currently installed software and not candidates
    ${EXPECTED_VERSION}=    Execute Command    dpkg -s tedge | grep "^Version: " | cut -d' ' -f2    strip=True
    ${VERSION}=    Regexp Escape    ${EXPECTED_VERSION}
    Device Should Have Installed Software    tedge,^${VERSION}::apt$        timeout=120

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    Set Test Variable    $DEVICE_SN
    Should Have MQTT Messages    te/device/main/service/tedge-mapper-c8y/status/health
    Execute Command    sudo start-http-server.sh

Stop tedge-agent
    [Timeout]                                5 seconds
    Stop Service                             tedge-agent

Custom Teardown
    Execute Command    sudo stop-http-server.sh
    Get Logs
