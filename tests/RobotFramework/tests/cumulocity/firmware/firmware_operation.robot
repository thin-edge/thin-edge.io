*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:firmware


*** Test Cases ***
Send firmware update operation from Cumulocity
    Should Have MQTT Messages    te/device/main///cmd/firmware_update    message_pattern=^\{\}$
    Cumulocity.Should Contain Supported Operations    c8y_Firmware

    ${operation}=    Cumulocity.Install Firmware    tedge-core    1.0.0    https://abc.com/some/firmware/url
    ${operation}=    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}
    Cumulocity.Device Should Have Firmware    tedge-core    1.0.0    https://abc.com/some/firmware/url


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    ThinEdgeIO.Transfer To Device    ${CURDIR}/firmware_update.toml    /etc/tedge/operations/
    Restart Service    tedge-agent
