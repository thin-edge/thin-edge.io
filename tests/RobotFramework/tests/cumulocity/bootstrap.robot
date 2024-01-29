*** Settings ***
Resource    ../../resources/common.resource

Library    Cumulocity
Library    ThinEdgeIO
Library    DateTime

Test Teardown    Get Logs

*** Test Cases ***

No unexpected child devices created with service autostart
    [Tags]    \#2584
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    Execute Command    systemctl start mosquitto
    Execute Command    systemctl start tedge-agent
    Execute Command    systemctl start tedge-mapper-c8y
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-install --no-secure || true
    Device Should Exist    ${DEVICE_SN}

    # wait for messages to be processed
    Sleep    15s

    # Assert that there are no child devices present.
    Cumulocity.Device Should Not Have Any Child Devices

No unexpected child devices created without service autostart
    [Tags]    \#2606
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}

    # Touching the operations directories should not create child devices
    Execute Command    touch /etc/tedge/operations
    Execute Command    touch /etc/tedge/operations/c8y

    # wait for fs event to be detected
    Sleep    5s

    # Assert that there are no child devices present.
    Cumulocity.Device Should Not Have Any Child Devices
