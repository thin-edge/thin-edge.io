*** Settings ***
Resource    ../../../resources/common.resource

Library    Cumulocity
Library    ThinEdgeIO
Library    DateTime

Test Teardown    Get Logs

*** Test Cases ***

test connect after starting all services
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    Execute Command    systemctl start mosquitto
    Execute Command    systemctl start tedge-agent
    Execute Command    systemctl start tedge-mapper-c8y
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-install --no-secure || true
    Device Should Exist    ${DEVICE_SN}

    # FIXME: Assert that there are no child devices present.
    # Cumulocity.Device Should Have A Child Devices    ""
