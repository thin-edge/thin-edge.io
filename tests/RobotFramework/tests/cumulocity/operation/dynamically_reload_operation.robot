*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:operation    theme:custom
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Custom operation successful
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command_1     /etc/tedge/operations/c8y/c8y_Command  
    ${operation}=    Cumulocity.Create Operation    description=do something    fragments={"c8y_Command":{"text":""}}
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    command 1\n

    # Change the custom operation (c8y_Command) file, c8y mapper has to execute the new command when its triggered
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command_2     /etc/tedge/operations/c8y/c8y_Command  
    ${operation}=    Cumulocity.Create Operation    description=do something    fragments={"c8y_Command":{"text":""}}
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    command 2\n
   

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/command_*.sh    /etc/tedge/operations/
    Execute Command    chmod a+x /etc/tedge/operations/command_*.sh