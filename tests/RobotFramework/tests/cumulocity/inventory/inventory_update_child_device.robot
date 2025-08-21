*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:c8y    theme:telemetry


*** Test Cases ***
Base inventory data
    ThinEdgeIO.Set Device Context    ${CHILD_SN}

    # Initial inventory.json data
    Execute Command
    ...    cmd=printf '{"name":"Advanced Child Device","type":"advancedV1"}\n' > /etc/tedge/device/inventory.json

    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

    Cumulocity.Device Should Have Fragment Values    name\="Advanced Child Device"
    Cumulocity.Device Should Have Fragment Values    type\="advancedV1"


*** Keywords ***
Custom Setup
    # Parent
    ${PARENT_SN}=    Setup    connect=${False}
    Set Suite Variable    $PARENT_SN

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}
    Execute Command    sudo tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883
    Execute Command    sudo tedge config set http.client.host ${PARENT_IP}

    ThinEdgeIO.Connect Mapper    c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y
    Cumulocity.Device Should Exist    ${PARENT_SN}

    # Child
    ${CHILD_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $CHILD_SN
    Set Suite Variable    $CHILD_XID    ${PARENT_SN}:device:${CHILD_SN}
    Setup Child Device
    Cumulocity.Device Should Exist    ${CHILD_XID}

Setup Child Device
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb packages/tedge-agent*.deb

    Execute Command    sudo tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set http.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

    Enable Service    tedge-agent
    Start Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

Custom Teardown
    Get Logs    ${PARENT_SN}
    Get Logs    ${CHILD_SN}
