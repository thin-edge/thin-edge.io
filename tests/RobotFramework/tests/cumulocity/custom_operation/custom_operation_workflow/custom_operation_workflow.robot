*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Variables ***
${PARENT_IP}    ${EMPTY}
${PARENT_SN}    ${EMPTY}
${CHILD_SN}     ${EMPTY}


*** Test Cases ***
Run custom operation with workflow execution
    Symlink Should Exist    /etc/tedge/operations/c8y/c8y_TakePicture
    Should Contain Supported Operations    c8y_TakePicture

    ${operation}=    Cumulocity.Create Operation
    ...    description=take a picture
    ...    fragments={"c8y_TakePicture":{"parameters": {"duration": "5s", "quality": "HD"}}}

    Verify Local Command    te/device/main///cmd/take_picture/#    executing
    Verify Local Command    te/device/main///cmd/take_picture/#    successful
    Operation Should Be SUCCESSFUL    ${operation}
    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_pattern=^(504|505|506),[0-9]+
    ...    minimum=2
    ...    maximum=2

Run custom operation with workflow execution on child device
    Child Setup
    Set Test Variable    $CHILD_XID    ${PARENT_SN}:device:${CHILD_SN}

    Set Device Context    ${PARENT_SN}
    Symlink Should Exist    /etc/tedge/operations/c8y/${CHILD_XID}/c8y_TakePicture

    Cumulocity.Device Should Exist    ${CHILD_XID}
    Should Contain Supported Operations    c8y_TakePicture

    ${operation}=    Cumulocity.Create Operation
    ...    description=take a picture
    ...    fragments={"c8y_TakePicture":{"parameters": {"duration": "5s", "quality": "HD"}}}

    Verify Local Command    te/device/${CHILD_SN}///cmd/take_picture/#    executing
    Verify Local Command    te/device/${CHILD_SN}///cmd/take_picture/#    successful
    Operation Should Be SUCCESSFUL    ${operation}
    Should Have MQTT Messages
    ...    c8y/s/us/${CHILD_XID}
    ...    message_pattern=^(504|505|506),[0-9]+
    ...    minimum=2
    ...    maximum=2


*** Keywords ***
Verify Local Command
    [Arguments]    ${topic}    ${expected_status}
    Should Have MQTT Messages
    ...    ${topic}
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="status":"${expected_status}"

Trasfer Configuration Files
    Transfer To Device    ${CURDIR}/c8y_TakePicture.template    /etc/tedge/operations/c8y/
    Transfer To Device    ${CURDIR}/take_picture.toml    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/take_picture.sh    /etc/tedge/operations/
    Execute Command    chmod a+x /etc/tedge/operations/take_picture.sh

Child Setup
    ${child_sn}=    Setup    skip_bootstrap=True
    Set Suite Variable    $CHILD_SN    ${child_sn}
    Set Device Context    ${CHILD_SN}

    Execute Command    dpkg -i packages/tedge_*.deb
    Execute Command    dpkg -i packages/tedge-agent_*.deb

    Execute Command    tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    tedge config set mqtt.client.port 1883
    Execute Command    tedge config set http.client.host ${PARENT_IP}
    Execute Command    tedge config set mqtt.topic_root te
    Execute Command    tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

    Trasfer Configuration Files

    Start Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=False
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}

    Set Device Context    ${PARENT_SN}
    Trasfer Configuration Files
    Execute Command    tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    tedge config set mqtt.external.bind.port 1883
    Execute Command    tedge reconnect c8y

    Device Should Exist    ${PARENT_SN}
