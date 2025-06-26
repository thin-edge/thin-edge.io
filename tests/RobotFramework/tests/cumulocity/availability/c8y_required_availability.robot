*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Test Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:monitoring


*** Test Cases ***
c8y_RequiredAvailability is set by default to an hour
    Execute Command    tedge connect c8y

    # Main
    Device Should Exist    ${DEVICE_SN}
    Device Should Have Fragment Values    c8y_RequiredAvailability.responseInterval\=60

    # Child
    Register child
    Device Should Have Fragment Values    c8y_RequiredAvailability.responseInterval\=60

c8y_RequiredAvailability is set with custom value
    # Set tedge config value before connecting
    Execute Command    sudo tedge config set c8y.availability.interval 0
    Execute Command    tedge connect c8y

    # Main
    Device Should Exist    ${DEVICE_SN}
    Device Should Have Fragment Values    c8y_RequiredAvailability.responseInterval\=0

    # Child
    Register child
    Device Should Have Fragment Values    c8y_RequiredAvailability.responseInterval\=0

c8y_RequiredAvailability is not set when disabled
    # Set tedge config value before connecting
    Execute Command    sudo tedge config set c8y.availability.enable false
    Execute Command    tedge connect c8y

    # Main
    Device Should Exist    ${DEVICE_SN}
    Managed Object Should Not Have Fragments    c8y_RequiredAvailability

    # Child
    Register child
    Managed Object Should Not Have Fragments    c8y_RequiredAvailability


*** Keywords ***
Test Setup
    ${DEVICE_SN}=    Setup    connect=${False}
    Set Test Variable    $DEVICE_SN

    ${CHILD_SN}=    Get Random Name
    Set Test Variable    $CHILD_SN
    Set Test Variable    $CHILD_XID    ${DEVICE_SN}:device:${CHILD_SN}

Register child
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device"}'
    Set Device    ${CHILD_XID}
    Device Should Exist    ${CHILD_XID}
