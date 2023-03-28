*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:troubleshooting    theme:plugins
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Install c8y-remote-access-plugin
    Skip    remote-access is not yet part of the official release. Enable when it is
    Device Should Have Installed Software    c8y-remote-access-plugin
    File Should Exist    /etc/tedge/operations/c8y/c8y_RemoteAccessConnect
    Execute Command    dpkg -r c8y-remote-access-plugin
    File Should Not Exist    /etc/tedge/operations/c8y/c8y_RemoteAccessConnect

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Execute Command    find /setup -type f -name "c8y-remote-access-plugin_*.deb" -exec dpkg -i {} \\;
    Restart Service    tedge-mapper-c8y
    Execute Command    systemctl start sshd
