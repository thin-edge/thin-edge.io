*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Test Teardown

Test Tags           theme:c8y    theme:troubleshooting    theme:childdevices


*** Test Cases ***
Support restarting the device
    Set Restart Command    ["/usr/bin/on_shutdown.sh", "1"]
    Set Restart Timeout    default
    Cumulocity.Should Contain Supported Operations    c8y_Restart
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180

Restart operation should be set to failed when an non-existent command is configured
    Set Restart Command    ["/usr/bin/on_shutdown_does_not_exist.sh"]
    Set Restart Timeout    60
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=Restart Failed: Fail to trigger a restart.*
    ...    timeout=30

Restart operation should be set to failed when command is not allowed by sudo
    Set Restart Command    ["/sbin/reboot"]
    Set Restart Timeout    60
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=Restart Failed: Fail to trigger a restart.*
    ...    timeout=30

Restart operation should be set to failed when the command does not restart the device
    Set Restart Command    ["/usr/bin/on_shutdown_no_reboot.sh"]
    Set Restart Timeout    30
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=Restart Failed: No shutdown has been triggered.*
    ...    timeout=60

Restart operation should be set to failed if the restart command times out
    [Documentation]    tedge should protect against commands which don't finish within a given time period (protect against hanging scripts)
    Set Restart Command    ["/usr/bin/on_shutdown.sh", "300"]
    Set Restart Timeout    5
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=Restart Failed: Restart command still running after 5 seconds
    ...    timeout=30

Restart operation should be set to failed when the command has been killed by a signal
    [Documentation]    If the restart command is killed, assume it did trigger a restart, and wait for the restart. Only fail the
    ...    the operation if the "wait for restart" logic does not detect a restart. This is because sometimes a shutdown
    ...    can trigger the script to be killed before it has a chance to exit successfully.
    Set Restart Command    ["/usr/bin/on_shutdown_signal_interrupt.sh"]
    Set Restart Timeout    30
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=Restart Failed: No shutdown has been triggered
    ...    timeout=60

Default restart timeout supports the default 60 second delay of the linux shutdown command
    [Documentation]    The shutdown -r command performs a device restart 60 seconds after being called. thin-edge.io should
    ...    support this setting out of the box
    Set Restart Command    ["shutdown", "-r"]
    Set Restart Timeout    default
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180


*** Keywords ***
Setup Child Device
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb

    Execute Command    sudo tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set mqtt.topic_root te
    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

    # Install plugin after the default settings have been updated to prevent it from starting up as the main plugin
    Execute Command    sudo dpkg -i packages/tedge-agent*.deb
    Execute Command    sudo systemctl enable tedge-agent
    Execute Command    sudo systemctl start tedge-agent

    Transfer To Device    ${CURDIR}/*.sh    /usr/bin/
    Transfer To Device    ${CURDIR}/*.service    /etc/systemd/system/
    Execute Command
    ...    cmd=chmod a+x /usr/bin/*.sh && chmod 644 /etc/systemd/system/*.service && systemctl enable on_startup.service
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh, /usr/bin/on_shutdown_no_reboot.sh, /usr/bin/on_shutdown_does_not_exist.sh, /usr/bin/on_shutdown_does_not_exist.sh, /usr/bin/on_shutdown_signal_interrupt.sh, !/sbin/reboot' > /etc/sudoers.d/tedge
    Set Restart Command    ["/usr/bin/on_shutdown.sh"]
    Set Restart Timeout    default

    # WORKAROUND: Uncomment next line once https://github.com/thin-edge/thin-edge.io/issues/2253 has been resolved
    # ThinEdgeIO.Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

Custom Setup
    # Parent
    ${parent_sn}=    Setup    connect=${False}
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}
    Execute Command    sudo tedge config set c8y.enable.log_upload true
    Execute Command    sudo tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883

    ThinEdgeIO.Connect Mapper    c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

    # Child
    ${CHILD_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $CHILD_SN
    Set Suite Variable    $CHILD_XID    ${PARENT_SN}:device:${CHILD_SN}
    Setup Child Device
    Cumulocity.Device Should Exist    ${CHILD_XID}

Test Teardown
    Get Logs    name=${PARENT_SN}
    Get Logs    name=${CHILD_SN}
