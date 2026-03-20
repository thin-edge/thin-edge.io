*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:flows


*** Test Cases ***
Fails when an empty flows list is found
    ${tmpdir}=    Create Temp Directory
    ${output}=    Execute Command    cmd=tedge flows list --flows-dir ${tmpdir} 2>&1    exp_exit_code=1
    Should Contain    ${output}    No valid flows

Fails when an invalid flow.toml file is found
    ${tmpdir}=    Create Temp Directory
    Execute Command    cmd=echo "[invalid" > ${tmpdir}/flow.toml
    ${output}=    Execute Command    cmd=tedge flows list --flows-dir ${tmpdir} 2>&1    exp_exit_code=1
    Should Contain    ${output}    Some invalid flows

Fails when an invalid javascript file is found
    ${tmpdir}=    Create Temp Directory
    Execute Command    cmd=echo 'export function onMessage(message, context) { invalid' > ${tmpdir}/main.js
    Execute Command    cmd=printf 'input.mqtt.topics = ["foo"]\nsteps = [{script = "main.js"}]' > ${tmpdir}/flow.toml
    ${output}=    Execute Command    cmd=tedge flows list --flows-dir ${tmpdir} 2>&1    exp_exit_code=1
    Should Contain    ${output}    Some invalid flows

Passes when all flows in a folder are valid
    ${tmpdir}=    Create Temp Directory
    Execute Command    cmd=echo 'export function onMessage(message, context) { return []; }' > ${tmpdir}/main.js
    Execute Command    cmd=printf 'input.mqtt.topics = ["foo"]\nsteps = [{script = "main.js"}]\n' > ${tmpdir}/flow.toml
    ${output}=    Execute Command    cmd=tedge flows list --flows-dir ${tmpdir} 2>&1    exp_exit_code=0


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

Create Temp Directory
    ${tmpdir}=    Execute Command    cmd=mktemp -d    strip=${True}
    RETURN    ${tmpdir}
