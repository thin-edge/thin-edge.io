*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:operation    theme:custom


*** Test Cases ***
c8y_RelayArray operation
    Cumulocity.Should Contain Supported Operations    c8y_RelayArray
    ${operation}=    Cumulocity.Create Operation
    ...    description=Set relays
    ...    fragments={"c8y_RelayArray":["OPEN", "CLOSED"]}
    Operation Should Be SUCCESSFUL    ${operation}
    Cumulocity.Managed Object Should Have Fragment Values    c8y_RelayArray\=["OPEN", "CLOSED"]


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/set_relay.sh    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_RelayArray    /etc/tedge/operations/c8y/c8y_RelayArray
