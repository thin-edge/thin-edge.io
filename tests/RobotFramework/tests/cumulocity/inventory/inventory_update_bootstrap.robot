*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:telemetry


*** Test Cases ***
Main device name and type not updated on mapper and agent restart
    Execute Command    tedge connect c8y
    Device Should Exist    ${DEVICE_SN}

    # Check initial values from registration
    Device Should Have Fragment Values    name\="Advanced ${DEVICE_SN}"
    Device Should Have Fragment Values    type\="advancedV1"

    # The original values should also be retained after restarting
    Execute Command    tedge reconnect c8y
    Sleep    3s
    Device Should Have Fragment Values    name\="Advanced ${DEVICE_SN}"
    Device Should Have Fragment Values    type\="advancedV1"

    # Change the values via the inventory.json
    Execute Command
    ...    cmd=printf '{"name":"Super Advanced Device","type":"advancedV2"}\n' > /etc/tedge/device/inventory.json
    Restart Service    tedge-agent
    Execute Command    tedge reconnect c8y
    Sleep    3s
    Device Should Have Fragment Values    name\="Super Advanced Device"
    Device Should Have Fragment Values    type\="advancedV2"

    # Override the cloud values by explicitly publishing new values on the local broker
    Execute Command    tedge mqtt pub --retain "te/device/main///twin/name" '"RaspberryPi 0001"'
    Execute Command    tedge mqtt pub --retain "te/device/main///twin/type" '"RaspberryPi5"'
    Sleep    3s
    Device Should Have Fragment Values    name\="RaspberryPi 0001"
    Device Should Have Fragment Values    type\="RaspberryPi5"
    Restart Service    tedge-agent
    Execute Command    tedge reconnect c8y
    Sleep    3s
    Device Should Have Fragment Values    name\="RaspberryPi 0001"
    Device Should Have Fragment Values    type\="RaspberryPi5"


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    ThinEdgeIO.Register Device With Cumulocity CA
    ...    external_id=${DEVICE_SN}
    ...    name=Advanced ${DEVICE_SN}
    ...    device_type=advancedV1
    Set Suite Variable    $DEVICE_SN
