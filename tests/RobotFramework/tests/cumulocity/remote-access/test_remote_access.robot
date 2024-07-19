*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:troubleshooting    theme:plugins
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Install/uninstall c8y-remote-access-plugin
    Device Should Have Installed Software    c8y-remote-access-plugin
    File Should Exist    /etc/tedge/operations/c8y/c8y_RemoteAccessConnect
    Execute Command    dpkg -r c8y-remote-access-plugin
    File Should Not Exist    /etc/tedge/operations/c8y/c8y_RemoteAccessConnect

Execute ssh command using PASSTHROUGH
    ${KEY_FILE}=    Configure SSH
    Add Remote Access Passthrough Configuration
    ${stdout}=    Execute Remote Access Command    command=tedge --version    exp_exit_code=0    user=root    key_file=${KEY_FILE}
    Should Match Regexp    ${stdout}    tedge .+

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Enable Service    ssh
    Start Service    ssh
