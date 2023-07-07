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

# TODO: make it not flaky
tedge-agent should terminate on SIGINT while downloading file
    # we download a file which is 500M, but tmpfs at /tmp is only 64M, so we
    # have to change tmp.path to be able to store the download
    Execute Command                          chmod 777 /root
    Execute Command                          tedge config set tmp.path /root
    Restart Service                          tedge-agent
    ${OPERATION}=    Install Software        test-very-large-software,1.0,https://t493319102.eu-latest.cumulocity.com/inventory/binaries/28057693

    # waiting for the download to start (so, for "Downloading: ...") to appear
    # in the log, but I have no clue how to do "wait until log contains ..."
    Operation Should Not Be PENDING          ${OPERATION}

    # timeout of 5s would be nice
    Stop tedge-agent

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

Stop tedge-agent
    [Timeout]                                5 seconds
    Stop Service                             tedge-agent
