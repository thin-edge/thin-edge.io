*** Settings ***
Resource    ../../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:operation    theme:custom
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Custom operation successful
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command_Success      /etc/tedge/operations/c8y/c8y_Command    
    ThinEdgeIO.Restart Service    tedge-mapper-c8y
    ${operation}=    Cumulocity.Create Operation    description=do something    fragments={"c8y_Command":{"text":"sleep 5"}}
    Operation Should Be SUCCESSFUL    ${operation}


Custom operation fails
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command_Fails      /etc/tedge/operations/c8y/c8y_Command   
    ThinEdgeIO.Restart Service    tedge-mapper-c8y
    ${operation}=    Cumulocity.Create Operation    description=do something    fragments={"c8y_Command":{"text":"sleep 20"}}
    Operation Should Be FAILED    ${operation}    failure_reason=operation failed due to timeout: duration=10s


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/customop_handler.*    /etc/tedge/operations/
    ThinEdgeIO.Restart Service    tedge-mapper-c8y
    ThinEdgeIO.Disconnect Then Connect Mapper    c8y
    