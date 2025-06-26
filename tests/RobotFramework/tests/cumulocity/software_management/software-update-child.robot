*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:c8y    theme:software    theme:childdevices


*** Test Cases ***
Supported software types should be declared during startup
    ThinEdgeIO.Set Device Context    ${PARENT_SN}
    Should Have MQTT Messages
    ...    topic=te/device/${CHILD_SN}///cmd/software_list
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="types":["apt"]
    Should Have MQTT Messages
    ...    topic=te/device/${CHILD_SN}///cmd/software_update
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="types":["apt"]

Software list should be populated during startup
    Cumulocity.Should Contain Supported Operations    c8y_SoftwareUpdate
    Device Should Have Installed Software    tedge    timeout=120

Install software via Cumulocity
    ${OPERATION}=    Install Software    rolldice
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software    rolldice


*** Keywords ***
Setup Child Device
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb

    Execute Command    sudo tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set mqtt.topic_root te
    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

    # Install plugin after the default settings have been updated to prevent it from starting up as the main plugin
    Execute Command    sudo dpkg -i packages/tedge-agent*.deb
    Execute Command    sudo dpkg -i packages/tedge-apt-plugin*.deb
    Execute Command    sudo systemctl enable tedge-agent
    Execute Command    sudo systemctl start tedge-agent

    # WORKAROUND: Uncomment next line once https://github.com/thin-edge/thin-edge.io/issues/2253 has been resolved
    # ThinEdgeIO.Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

Custom Setup
    # Parent
    ${parent_sn}=    Setup    connect=${False}
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}
    Execute Command    sudo tedge config set c8y.enable.log_upload true
    Execute Command    sudo tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883

    ThinEdgeIO.Connect Mapper    c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

    # Child
    ${CHILD_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $CHILD_SN
    Set Suite Variable    $CHILD_XID    ${PARENT_SN}:device:${CHILD_SN}
    Setup Child Device
    Cumulocity.Device Should Exist    ${CHILD_XID}

Custom Teardown
    Get Logs    name=${PARENT_SN}
    Get Logs    name=${CHILD_SN}
