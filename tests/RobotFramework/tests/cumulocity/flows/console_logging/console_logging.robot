*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:flows


*** Test Cases ***
Flows can log message at different log levels
    ${name}=    Get Random Name    prefix=flows-
    Transfer To Device    ${CURDIR}/logger/flow.toml    dst=/var/share/${name}/logger/
    Transfer To Device    ${CURDIR}/logger/main.js    dst=/var/share/${name}/logger/
    ${output}=    Execute Command
    ...    cmd=echo '[logger] test' | tedge flows test --flows-dir /var/share/${name}/logger 2>&1
    ...    exp_exit_code=0
    Should Contain    ${output}    message using 'console.log'
    Should Contain    ${output}    message using 'console.debug'
    Should Contain    ${output}    message using 'console.info'
    Should Contain    ${output}    message using 'console.warn'
    Should Contain    ${output}    message using 'console.error'


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
