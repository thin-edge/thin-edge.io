*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO
Library    DebugLibrary

Test Tags    theme:c8y    theme:troubleshooting
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Supports restarting the device
    [Documentation]    Use a longer timeout period to allow the device time to restart and allow the initial token fetching process to fail at least one (due to the 60 seconds retry window)
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Transfer To Device    ${CURDIR}/*.sh    /usr/bin/
    Transfer To Device    ${CURDIR}/*.service    /etc/systemd/system/
    Execute Command    apt-get install -y systemd-sysv && chmod a+x /usr/bin/*.sh && chmod 644 /etc/systemd/system/*.service && systemctl enable on_startup.service
    Execute Command    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh' > /etc/sudoers.d/tedge
    Execute Command    cmd=sed -i 's|reboot =.*|reboot = ["/usr/bin/on_shutdown.sh"]|g' /etc/tedge/system.toml
