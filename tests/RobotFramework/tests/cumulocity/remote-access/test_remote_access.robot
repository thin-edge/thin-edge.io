*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Test Cases ***
Install/uninstall c8y-remote-access-plugin
    Device Should Have Installed Software    c8y-remote-access-plugin
    File Should Exist    /etc/tedge/operations/c8y/c8y_RemoteAccessConnect

    # Check that a command is used instead of an explicit path #3111
    ${executable}=    Execute Command
    ...    cmd=grep "command" /etc/tedge/operations/c8y/c8y_RemoteAccessConnect | cut -d= -f2-
    ...    strip=${True}
    ...    timeout=5
    Should Match Regexp    ${executable}    pattern=^"c8y-remote-access-plugin\\b

    Execute Command    dpkg -r c8y-remote-access-plugin
    File Should Not Exist    /etc/tedge/operations/c8y/c8y_RemoteAccessConnect

Init c8y-remote-access-plugin with the custom user and group
    Device Should Have Installed Software    c8y-remote-access-plugin

    Execute Command    sudo c8y-remote-access-plugin --init --user petertest --group petertest
    Path Should Have Permissions    /etc/tedge/operations/c8y    mode=755    owner_group=petertest:petertest
    Path Should Have Permissions
    ...    /etc/tedge/operations/c8y/c8y_RemoteAccessConnect
    ...    mode=644
    ...    owner_group=petertest:petertest

Execute ssh command using PASSTHROUGH
    ${KEY_FILE}=    Configure SSH
    Add Remote Access Passthrough Configuration
    ${stdout}=    Execute Remote Access Command
    ...    command=tedge --version
    ...    exp_exit_code=0
    ...    user=root
    ...    key_file=${KEY_FILE}
    Should Match Regexp    ${stdout}    tedge .+

Remote access session is independent from mapper (when using socket activation)
    ${KEY_FILE}=    Configure SSH
    Add Remote Access Passthrough Configuration

    # Restart mapper via tedge reconnect
    ${stdout}=    Execute Remote Access Command
    ...    command=tedge reconnect c8y
    ...    exp_exit_code=0
    ...    user=root
    ...    key_file=${KEY_FILE}
    Should Contain    ${stdout}    tedge-agent service successfully started and enabled
    Cumulocity.Should Only Have Completed Operations

    # Transfer a test script to the device to reduce errors with complex one-liners
    Transfer To Device    ${CURDIR}/restart-service.sh    /usr/local/share/tests/

    # Restart agent
    ${stdout}=    Execute Remote Access Command
    ...    command=/usr/local/share/tests/restart-service.sh "tedge-agent" "restarted-tedge-agent"
    ...    exp_exit_code=0
    ...    user=root
    ...    key_file=${KEY_FILE}
    Should Contain    ${stdout}    restarted-tedge-agent
    Cumulocity.Should Only Have Completed Operations

Remote access session is not independent from mapper (when socket activation is not available)
    ${KEY_FILE}=    Configure SSH
    Add Remote Access Passthrough Configuration

    # Stop the socket
    Execute Command    systemctl stop c8y-remote-access-plugin.socket

    # Check a command which won't stop the connection first
    ${stdout}=    Execute Remote Access Command
    ...    command=tedge config get device.id
    ...    exp_exit_code=0
    ...    user=root
    ...    key_file=${KEY_FILE}
    Should Be Equal    ${stdout}    ${DEVICE_SN}    strip_spaces=${True}

    ${stdout}=    Execute Remote Access Command
    ...    command=tedge reconnect c8y
    ...    exp_exit_code=255
    ...    user=root
    ...    key_file=${KEY_FILE}
    Should Not Contain
    ...    ${stdout}
    ...    tedge-agent service successfully started and enabled
    ...    msg=This message should not be present as the connection will be severed due to being a child process of the tedge-mapper-c8y
    Cumulocity.Should Only Have Completed Operations


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Enable Service    ssh
    Start Service    ssh
