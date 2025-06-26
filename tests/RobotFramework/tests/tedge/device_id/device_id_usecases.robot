*** Settings ***
Documentation       This suite covers all use-cases in the issue #3369

Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Teardown      Get Suite Logs
Test Setup          Custom Setup

Test Tags           theme:c8y    theme:cli


*** Test Cases ***
Use explicit device id during cert creation
    Execute Command    tedge config set device.id ${FOO}
    ${output}=    Execute Command    tedge cert create --device-id ${BAR}
    Should Contain    ${output}    CN=${BAR}

    ${output}=    Execute Command    tedge config get device.id    strip=${True}
    Should Be Equal    ${output}    ${BAR}

    ${output}=    Execute Command    tedge connect c8y    ignore_exit_code=${True}    stdout=${False}    stderr=${True}
    Should Contain    ${output}    device id: ${BAR}

Use default device.id
    Execute Command    tedge config set device.id ${FOO}
    ${output}=    Execute Command    tedge cert create
    Should Contain    ${output}    CN=${FOO}

    ${output}=    Execute Command    tedge config get device.id    strip=${True}
    Should Be Equal    ${output}    ${FOO}

    ${output}=    Execute Command    tedge connect c8y    ignore_exit_code=${True}    stdout=${False}    stderr=${True}
    Should Contain    ${output}    device id: ${FOO}

Use device id from cert
    Execute Command    tedge cert create --device-id ${FOO}
    Execute Command    tedge config set device.id ${BAR}

    ${output}=    Execute Command    tedge config get device.id    strip=${True}
    Should Be Equal    ${output}    ${FOO}

    ${output}=    Execute Command    tedge connect c8y    ignore_exit_code=${True}    stdout=${False}    stderr=${True}
    Should Contain    ${output}    device id: ${FOO}

Use default device.id to create the cert
    Execute Command    tedge config set device.id ${FOO}

    ${output}=    Execute Command    tedge cert create
    Should Contain    ${output}    CN=${FOO}

    Execute Command    tedge config set device.id ${BAR}

    ${output}=    Execute Command    tedge config get device.id    strip=${True}
    Should Be Equal    ${output}    ${FOO}

    ${output}=    Execute Command    tedge connect c8y    ignore_exit_code=${True}    stdout=${False}    stderr=${True}
    Should Contain    ${output}    device id: ${FOO}


*** Keywords ***
Custom Setup
    ${device_sn}=    Setup    connect=${False}
    Set Test Variable    $FOO    ${device_sn}-1
    Set Test Variable    $BAR    ${device_sn}-2
    Execute Command    tedge cert remove
