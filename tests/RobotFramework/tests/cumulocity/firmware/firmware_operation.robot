*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:firmware    theme:plugins
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Successful firmware operation
    ${operation}=    Cumulocity.Install Firmware    ubuntu    1.0.2    https://dummy.url/firmware.zip
    ${operation}=    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Device Should Have Firmware    ubuntu    1.0.2    https://dummy.url/firmware.zip

Install with empty firmware name
    ${operation}=    Cumulocity.Install Firmware    ${EMPTY}    1.0.2    https://dummy.url/firmware.zip
    Operation Should Be FAILED    ${operation}    failure_reason=.*Invalid firmware name. Firmware name cannot be empty    timeout=120


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/firmware_handler.*    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Firmware*         /etc/tedge/operations/c8y/
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Disconnect Then Connect Mapper    c8y
