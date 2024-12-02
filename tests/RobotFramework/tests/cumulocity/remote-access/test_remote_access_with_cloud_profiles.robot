*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins    theme:remoteaccess    adapter:docker


*** Test Cases ***
Execute ssh command with a cloud-profile enabled mapper
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
    Execute Command    sed -i 's/\\[c8y\\]/\[c8y.profiles.test\]/g' /etc/tedge/tedge.toml
    Execute Command    tedge disconnect c8y
    Execute Command    tedge connect c8y@test    timeout=0
