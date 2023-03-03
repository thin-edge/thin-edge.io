*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:configuration
Suite Setup            Custom Setup
Suite Teardown         Get Logs


*** Test Cases ***
Set keys should return value on stdout
    ${output}=    Execute Command    tedge config get device.id 2>/dev/null    strip=True
    Should Be Equal    ${output}    ${DEVICE_SN}

Unset keys should not return anything on stdout and warnings on stderr
    ${output}=    Execute Command    tedge config get az.url 2>/dev/null    exp_exit_code=0
    Should Be Empty    ${output}

    ${stderr}=    Execute Command    tedge config get az.url 2>&1 >/dev/null    exp_exit_code=0
    Should Not Be Empty    ${stderr}

Invalid keys should not return anything on stdout and warnings on stderr
    ${output}=    Execute Command    tedge config get does.not.exist 2>/dev/null    exp_exit_code=!0
    Should Be Empty    ${output}

    ${stderr}=    Execute Command    tedge config get does.not.exist 2>&1 >/dev/null    exp_exit_code=!0
    Should Not Be Empty    ${stderr}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
