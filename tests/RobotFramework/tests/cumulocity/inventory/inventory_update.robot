*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:telemetry
Suite Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Update Inventory data via inventory.json
    ${mo}=    Cumulocity.Device Should Have Fragments    customData    types
    Should Be Equal    ${mo["customData"]["mode"]}    ACTIVE
    Should Be Equal As Integers    ${mo["customData"]["version"]}    ${1}
    Should Be Equal    ${mo["customData"]["alertingEnabled"]}    ${True}
    Should Be Equal    ${mo["types"][0]}    type1
    Should Be Equal    ${mo["types"][1]}    type2

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=                    Setup
    Set Suite Variable               $DEVICE_SN
    Device Should Exist              ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/inventory.json    /etc/tedge/device/
    ThinEdgeIO.Disconnect Then Connect Mapper    c8y
