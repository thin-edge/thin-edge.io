*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins    theme:remoteaccess    adapter:docker


*** Test Cases ***
Execute ssh command with a custom root certificate path
    ${KEY_FILE}=    Configure SSH
    Add Remote Access Passthrough Configuration
    ${stdout}=    Execute Remote Access Command
    ...    command=echo foobar
    ...    exp_exit_code=0
    ...    user=root
    ...    key_file=${KEY_FILE}
    Should Contain    ${stdout}    foobar


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Enable Service    ssh
    Start Service    ssh
    Execute Command    mv /etc/ssl/certs /etc/ssl/moved-certs
    Execute Command    tedge config set c8y.root_cert_path /etc/ssl/moved-certs
