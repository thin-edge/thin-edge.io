*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:software    theme:plugins
Test Setup       Custom Setup
Test Teardown    Custom Teardown

*** Test Cases ***
Supported software types should be declared during startup
    [Documentation]    #2654 This test will be updated once advanced software management support is implemented
    Should Have MQTT Messages    topic=te/device/main///cmd/software_list    minimum=1    maximum=1    message_contains="types":["apt"]
    Should Have MQTT Messages    topic=te/device/main///cmd/software_update    minimum=1    maximum=1    message_contains="types":["apt"]

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
    ${VERSION}=    Escape Pattern    ${EXPECTED_VERSION}    is_json=${True}
    Device Should Have Installed Software    {"name": "tedge", "softwareType": "apt", "version": "${VERSION}"}    timeout=120

Manual software_list operation request
    # Note: There isn't a Cumulocity IoT operation related to getting the software list, so no need to check for operation transitions
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/software_list/local-1111
    ...    payload={"status":"init"}
    ...    expected_status=successful

Manual software_update operation request
    # Don't worry about the command failing, that is expected since the package to be installed does not exist
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/software_update/local-2222
    ...    payload={"status":"init","updateList":[{"type":"apt","modules":[{"name":"package-does-not-exist","version":"latest","action":"install"}]}]}
    ...    expected_status=failed
    ...    c8y_fragment=c8y_SoftwareUpdate


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

Publish and Verify Local Command
    [Arguments]    ${topic}    ${payload}    ${expected_status}=successful    ${c8y_fragment}=
    [Teardown]    Execute Command    tedge mqtt pub --retain '${topic}' ''
    Execute Command    tedge mqtt pub --retain '${topic}' '${payload}'
    ${messages}=    Should Have MQTT Messages    ${topic}    minimum=1    maximum=1    message_contains="status":"${expected_status}"

    Sleep    5s    reason=Given mapper a chance to react, if it does not react with 5 seconds it never will
    ${retained_message}    Execute Command    timeout 1 tedge mqtt sub --no-topic '${topic}'    ignore_exit_code=${True}    strip=${True}
    Should Be Equal    ${messages[0]}    ${retained_message}    msg=MQTT message should be unchanged

    IF    "${c8y_fragment}"
        # There should not be any c8y related operation transition messages sent: https://cumulocity.com/guides/reference/smartrest-two/#updating-operations
        Should Have MQTT Messages    c8y/s/ds    message_pattern=^(501|502|503),${c8y_fragment}.*    minimum=0    maximum=0
    END
