*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:flows


*** Test Cases ***
Flow service is enabled by default
    ThinEdgeIO.Service Should Be Enabled    tedge-flows
    ThinEdgeIO.Service Should Be Running    tedge-flows


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    # Restart service after bootstrapping in case if mqtt client auth has changed
    Restart Service    tedge-flows
