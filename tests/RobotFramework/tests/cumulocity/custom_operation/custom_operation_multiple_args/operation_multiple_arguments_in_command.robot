*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:operation    theme:custom


*** Test Cases ***
Run the custom operation with multiple arguments
    ${operation}=    Cumulocity.Create Operation    description=do something    fragments={"c8y_Command":{"text":""}}
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    command 1\n


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/command_1.sh    /etc/tedge/operations/
    Execute Command    chmod a+x /etc/tedge/operations/command_1.sh
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command_1    /etc/tedge/operations/c8y/c8y_Command
