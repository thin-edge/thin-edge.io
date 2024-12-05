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
    Cumulocity.Should Contain Supported Operations    c8y_TakePicture

    ${operation}=    Cumulocity.Create Operation
    ...    description=take a picture
    ...    fragments={"c8y_TakePicture":{"parameters": {"duration": "5s", "quality": "HD"}}}
    Verify Local Command    main    take_picture

    Should Have MQTT Messages
    ...    c8y/s/us
    ...    minimum=1
    ...    maximum=1
    ...    message_pattern=^506,[0-9]+,(5s HD)
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}

Add template and workflow file dynamically
    Transfer To Device    ${CURDIR}/c8y_OpenDoor.template    /etc/tedge/operations/c8y/
    Transfer To Device    ${CURDIR}/open_door.toml    /etc/tedge/operations/

    Should Have MQTT Messages
    ...    te/device/main///cmd/open_door
    ...    pattern="^{}$"
    Symlink Should Exist    /etc/tedge/operations/c8y/c8y_OpenDoor
    Cumulocity.Should Contain Supported Operations    c8y_OpenDoor

    ${operation}=    Cumulocity.Create Operation
    ...    description=open a door
    ...    fragments={"c8y_OpenDoor":{"delay": "5s", "user": "cat"}}
    Verify Local Command    main    open_door
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}

Run custom operation with workflow execution on child device
    Child Setup
    Set Test Variable    $CHILD_XID    ${PARENT_SN}:device:${CHILD_SN}

    Set Device Context    ${PARENT_SN}
    Symlink Should Exist    /etc/tedge/operations/c8y/${CHILD_XID}/c8y_TakePicture

    Cumulocity.Device Should Exist    ${CHILD_XID}
    Cumulocity.Should Contain Supported Operations    c8y_TakePicture

    ${operation}=    Cumulocity.Create Operation
    ...    description=take a picture
    ...    fragments={"c8y_TakePicture":{"parameters": {"duration": "5s", "quality": "HD"}}}

    Verify Local Command    ${CHILD_SN}    take_picture
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Verify Local Command
    [Arguments]    ${device}    ${cmd}
    Should Have MQTT Messages
    ...    te/device/${device}///cmd/${cmd}/#
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="status":"executing"
    Should Have MQTT Messages
    ...    te/device/${device}///cmd/${cmd}/#
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="status":"successful"

Transfer Configuration Files
    Transfer To Device    ${CURDIR}/c8y_TakePicture.template    /etc/tedge/operations/c8y/
    Transfer To Device    ${CURDIR}/take_picture.toml    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/do_something.sh    /etc/tedge/operations/
    Execute Command    chmod a+x /etc/tedge/operations/do_something.sh

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

    Transfer Configuration Files

    Start Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=False
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}

    Set Device Context    ${PARENT_SN}
    Transfer Configuration Files
    Execute Command    tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    tedge config set mqtt.external.bind.port 1883
    Execute Command    tedge reconnect c8y

    Cumulocity.Device Should Exist    ${PARENT_SN}
