*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO
Library             ../../.venv/lib/python3.11/site-packages/robot/libraries/String.py

Suite Teardown      Get Logs
Test Setup          Custom Setup

Test Tags           theme:c8y    theme:cli


*** Test Cases ***
Certificate's CN must match device.id
    ${cert_output}=    Execute Command    tedge cert show
    Should Contain    ${cert_output}    ${DEVICE_SN}

    Execute Command    tedge config set device.id foo
    ${output}=    Execute Command    tedge connect c8y
    ...    exp_exit_code=1
    ...    stdout=False
    ...    stderr=True
    ...    timeout=0
    ...    strip=True
    Should Be Equal
    ...    ${output}
    ...    error: device.id 'foo' mismatches to the device certificate's CN '${DEVICE_SN}'


*** Keywords ***
Custom Setup
    ${device_sn}=    Setup    skip_bootstrap=${True}
    Execute Command    ./bootstrap.sh --no-connect
    Set Test Variable    $DEVICE_SN    ${device_sn}
