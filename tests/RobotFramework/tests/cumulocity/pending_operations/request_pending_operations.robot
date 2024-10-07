*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:operation


*** Test Cases ***
Process any pending operations after connection disruptions
    ThinEdgeIO.Bridge Should Be Up    c8y

    # Disconnect (wait 2.5 times the MQTT keepalive interval which defaults to 60s)
    ThinEdgeIO.Disconnect From Network
    ThinEdgeIO.Bridge Should Be Down    c8y    timeout=180

    # Create cloud operation whilst the device is disconnected
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin

    # Restore connection
    ThinEdgeIO.Connect To Network
    ThinEdgeIO.Bridge Should Be Up    c8y

    # WORKAROUND: Request pending operations via SmartREST 2.0
    # Execute Command    tedge mqtt pub -q 1 c8y/s/us 500

    Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
