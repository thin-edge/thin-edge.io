*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Test Teardown

Test Tags           theme:c8y    theme:flows


*** Test Cases ***
tedge flows list should read flows.memory.stack_size
    Execute Command    cmd=tedge config set flows.memory.stack_size 1024
    ${output}=    Execute Command
    ...    cmd=tedge flows list --mapper local
    ...    exp_exit_code=1
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain    ${output}    Maximum call stack size exceeded
    Execute Command    cmd=tedge config unset flows.memory.stack_size
    ${output}=    Execute Command
    ...    cmd=tedge flows list --mapper local
    ...    exp_exit_code=0
    ...    stdout=${False}
    ...    stderr=${True}
    Should Not Contain    ${output}    Maximum call stack size exceeded

tedge flows list should read flows.memory.heap_size
    Execute Command    cmd=tedge config set flows.memory.heap_size 1024
    ${output}=    Execute Command
    ...    cmd=tedge flows list --mapper local
    ...    exp_exit_code=1
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain    ${output}    Allocation failed while creating object
    Execute Command    cmd=tedge config unset flows.memory.heap_size
    ${output}=    Execute Command
    ...    cmd=tedge flows list --mapper local
    ...    exp_exit_code=0
    ...    stdout=${False}
    ...    stderr=${True}
    Should Not Contain    ${output}    Allocation failed while creating object


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Execute Command    cmd=mkdir -p /etc/tedge/mappers/local/flows/custom-measurements
    Transfer To Device
    ...    src=${CURDIR}/custom-measurements.js
    ...    dst=/etc/tedge/mappers/local/flows/custom-measurements/
    Transfer To Device
    ...    src=${CURDIR}/custom-measurements.toml
    ...    dst=/etc/tedge/mappers/local/flows/custom-measurements/

Test Teardown
    Execute Command    cmd=tedge config unset flows.memory.stack_size
    Execute Command    cmd=tedge config unset flows.memory.heap_size
    Get Logs
