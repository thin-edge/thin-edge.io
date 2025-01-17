*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:c8y    theme:troubleshooting


*** Test Cases ***
Supports restarting the device
    [Documentation]    Use a longer timeout period to allow the device time to restart and allow the initial token fetching process to fail at least one (due to the 60 seconds retry window)
    Cumulocity.Should Contain Supported Operations    c8y_Restart
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180

Supports restarting the device without sudo and running as root
    Set Service User    tedge-agent    root
    Execute Command    mv /usr/bin/sudo /usr/bin/sudo.bak
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180

# Note: Sending an actual local restart operation to trigger a restart (e.g. status=init) is not feasible
# as the assertion would fail due to the device/container being unresponsive during the actual restart.
# Just checking that the messages do not trigger any c8y messages and does not clear the retained message

tedge-mapper-c8y does not react to local restart operations transitions
    [Template]    Publish and Verify Local Command
    topic=te/device/main///cmd/restart/local-1111    payload={"status":"executing"}    expected_status=executing    c8y_fragment=c8y_Restart
    topic=te/device/main///cmd/restart/local-2222    payload={"status":"failed"}    expected_status=failed    c8y_fragment=c8y_Restart
    topic=te/device/main///cmd/restart/local-3333    payload={"status":"successful"}    expected_status=successful    c8y_fragment=c8y_Restart


*** Keywords ***
Set Service User
    [Arguments]    ${SERVICE_NAME}    ${SERVICE_USER}
    Execute Command    mkdir -p /etc/systemd/system/${SERVICE_NAME}.service.d/
    Execute Command
    ...    cmd=printf "[Service]\nUser = ${SERVICE_USER}" | sudo tee /etc/systemd/system/${SERVICE_NAME}.service.d/10-user.conf
    Execute Command    systemctl daemon-reload
    Restart Service    ${SERVICE_NAME}

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Transfer To Device    ${CURDIR}/*.sh    /usr/bin/
    Transfer To Device    ${CURDIR}/*.service    /etc/systemd/system/
    Execute Command
    ...    chmod a+x /usr/bin/*.sh && chmod 644 /etc/systemd/system/*.service && systemctl enable on_startup.service
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh' > /etc/sudoers.d/tedge
    Set Restart Command    ["/usr/bin/on_shutdown.sh"]

Custom Teardown
    # Restore sudo in case if the tests are run on a device (and not in a container)
    Execute Command    [ -f /usr/bin/sudo.bak ] && mv /usr/bin/sudo.bak /usr/bin/sudo || true
    Get Logs

Publish and Verify Local Command
    [Arguments]    ${topic}    ${payload}    ${expected_status}=successful    ${c8y_fragment}=
    Execute Command    tedge mqtt pub --retain '${topic}' '${payload}'
    ${messages}=    Should Have MQTT Messages
    ...    ${topic}
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="status":"${expected_status}"

    Sleep    2s    reason=Given mapper a chance to react, if it does not react with 2 seconds it never will
    ${retained_message}=    Execute Command
    ...    timeout 1 tedge mqtt sub --no-topic '${topic}'
    ...    ignore_exit_code=${True}
    ...    strip=${True}
    Should Be Equal    ${messages[0]}    ${retained_message}    msg=MQTT message should be unchanged

    IF    "${c8y_fragment}"
        # There should not be any c8y related operation transition messages sent: https://cumulocity.com/docs/smartrest/mqtt-static-templates/#updating-operations
        Should Have MQTT Messages
        ...    c8y/s/us
        ...    message_pattern=^(501|502|503|504|505|506),${c8y_fragment}.*
        ...    minimum=0
        ...    maximum=0
    END
    [Teardown]    Execute Command    tedge mqtt pub --retain '${topic}' ''
