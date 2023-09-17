*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:troubleshooting
Test Setup    Custom Setup
Test Teardown    Custom Teardown

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


*** Keywords ***

Set Service User
    [Arguments]    ${SERVICE_NAME}    ${SERVICE_USER}
    Execute Command    mkdir -p /etc/systemd/system/${SERVICE_NAME}.service.d/
    Execute Command    cmd=printf "[Service]\nUser = ${SERVICE_USER}" | sudo tee /etc/systemd/system/${SERVICE_NAME}.service.d/10-user.conf
    Execute Command    systemctl daemon-reload
    Restart Service    ${SERVICE_NAME}

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Transfer To Device    ${CURDIR}/*.sh    /usr/bin/
    Transfer To Device    ${CURDIR}/*.service    /etc/systemd/system/
    Execute Command    apt-get install -y systemd-sysv && chmod a+x /usr/bin/*.sh && chmod 644 /etc/systemd/system/*.service && systemctl enable on_startup.service
    Execute Command    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh' > /etc/sudoers.d/tedge
    Execute Command    cmd=sed -i 's|reboot =.*|reboot = ["/usr/bin/on_shutdown.sh"]|g' /etc/tedge/system.toml

Custom Teardown
    # Restore sudo in case if the tests are run on a device (and not in a container)
    Execute Command    [ -f /usr/bin/sudo.bak ] && mv /usr/bin/sudo.bak /usr/bin/sudo || true
    Get Logs
