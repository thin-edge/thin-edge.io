*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO
Library             JSONLibrary
Library             String

Suite Setup         Custom Setup
Test Teardown       Get Logs    name=${PARENT_SN}

Force Tags          theme:firmware    theme:childdevices


*** Variables ***
${PARENT_IP}
${HTTP_PORT}                8000

${PARENT_SN}
${CHILD_SN}

${op_id}
${cache_key}
${file_url}
${file_transfer_url}
${file_creation_time}


*** Test Cases ***
Prerequisite Parent
    Set Device Context    ${PARENT_SN}    #Creates ssh connection to the parent device
    Execute Command    sudo tedge disconnect c8y

    Delete child related content    #Delete any previous created child related configuration files/folders on the parent device
    Check for child related content    #Checks if folders that will trigger child device creation are existing
    Set external MQTT bind address    #Setting external MQTT bind address which child will use for communication
    Set external MQTT port    #Setting external MQTT port which child will use for communication Default:1883

    Sleep    3s
    Execute Command    sudo tedge connect c8y
    Restart Firmware plugin    #Stop and Start c8y-firmware-plugin
    Cumulocity.Log Device Info

Prerequisite Child
    Child device delete firmware file    #Delete previous downloaded firmware file
    Create child device    #Let mapper to create a child device
    Validate child Name    #This is to check the existence of the bug: https://github.com/thin-edge/thin-edge.io/issues/1569

Child device firmware update
    Upload binary to Cumulocity
    Create c8y_Firmware operation
    Validate firmware update request
    Child device response on update request    #Child device is sending 'executing' and 'successful' MQTT responses
    Validate Cumulocity operation status and MO

Child device firmware update with cache
    Get timestamp of cache
    Create c8y_Firmware operation
    Validate firmware update request
    Child device response on update request    #Child device is sending 'executing' and 'successful' MQTT responses
    Validate Cumulocity operation status and MO
    Validate if file is not newly downloaded


*** Keywords ***
Delete child related content
    Execute Command    sudo rm -rf /etc/tedge/operations/c8y/TST*    #if folder exists, child device will be created
    Execute Command    sudo rm -rf /etc/tedge/c8y/TST*    #if folder exists, child device will be created
    Execute Command    sudo rm -rf /var/tedge/file-transfer/*
    Execute Command    sudo rm -rf /var/tedge/cache/*
    Execute Command    sudo rm -rf /var/tedge/firmware/*

Check for child related content
    Set Device Context    ${PARENT_SN}
    Directory Should Not Have Sub Directories    /etc/tedge/operations/c8y
    Directory Should Not Have Sub Directories    /etc/tedge/c8y
    Directory Should Not Have Sub Directories    /var/tedge/file-transfer
    Directory Should Be Empty    /var/tedge/cache
    Directory Should Be Empty    /var/tedge/firmware

Set external MQTT bind address
    Set Device Context    ${PARENT_SN}
    Execute Command    sudo tedge config set mqtt.external.bind.address ${PARENT_IP}

Set external MQTT port
    Set Device Context    ${PARENT_SN}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883

Restart Firmware plugin
    ThinEdgeIO.Restart Service    c8y-firmware-plugin.service

Child device delete firmware file
    Set Device Context    ${CHILD_SN}
    Execute Command    sudo rm -f firmware1

Create child device
    [Documentation]    FIXME: Without the first sleep after "Set Device Context", the device didn't get created.
    ...    The second sleep ensures that first 101 (creating child device object) is sent, then next 114 (declaring supported operation).
    ...    If the order is opposite, the child device name will start with "MQTT Device".
    Set Device Context    ${PARENT_SN}
    Sleep    3s
    Execute Command    mkdir -p /etc/tedge/operations/c8y/${CHILD_SN}
    Sleep    3s
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Firmware    /etc/tedge/operations/c8y/${CHILD_SN}/
    Cumulocity.Device Should Exist    ${CHILD_SN}

Validate child Name
    ${child_mo}=    Cumulocity.Device Should Have Fragments    name
    Should Be Equal    device_${PARENT_SN}    ${child_mo["owner"]}    # The parent is the owner of the child

Get timestamp of cache
    Set Device Context    ${PARENT_SN}
    ${file_creation_time}=    Execute Command    date -r /var/tedge/cache/${cache_key}
    Set Suite Variable    $file_creation_time

Upload binary to Cumulocity
    ${file_url}=    Cumulocity.Create Inventory Binary    firmware1.txt    firmware1    file=${CURDIR}/firmware1.txt
    Set Suite Variable    $file_url

Create c8y_Firmware operation
    ${operation}=    Cumulocity.Install Firmware    firmware1    1.0    ${file_url}
    Set Suite Variable    $operation
    Cumulocity.Operation Should Be DELIVERED    ${operation}

Validate if file is not newly downloaded
    Set Device Context    ${PARENT_SN}
    ${file_creation_time_new}=    Execute Command    date -r /var/tedge/cache/${cache_key}
    Should Be Equal    ${file_creation_time}    ${file_creation_time_new}

Validate firmware update request
    Set Device Context    ${PARENT_SN}
    ${listen}=    ThinEdgeIO.Should Have MQTT Messages
    ...    topic=tedge/${CHILD_SN}/commands/req/firmware_update
    ...    date_from=-5s
    ${message}=    JSONLibrary.Convert String To Json    ${listen[0]}

    Should Not Be Empty    ${message["id"]}
    Should Be Equal    ${message["attempt"]}    ${1}
    Should Be Equal    ${message["name"]}    firmware1
    Should Be Equal    ${message["version"]}    1.0
    Should Be Equal    ${message["sha256"]}    4b0126519dfc1a3023851bfcc5b312b20fc80452256f7f40a5d8722765500ba9
    Should Match Regexp
    ...    ${message["url"]}
    ...    ^http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/firmware_update/[0-9A-Za-z]+$

    ${cache_key}=    Get Regexp Matches
    ...    ${message["url"]}
    ...    ^http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/firmware_update/([0-9A-Za-z]+)$
    ...    1

    Set Suite Variable    $op_id    ${message["id"]}
    Set Suite Variable    $file_transfer_url    ${message["url"]}
    Set Suite Variable    $cache_key    ${cache_key[0]}

Child device response on update request
    Set Device Context    ${CHILD_SN}
    Set Test Variable    $topic_res    tedge/${CHILD_SN}/commands/res/firmware_update

    Execute Command    mosquitto_pub -h ${PARENT_IP} -t ${topic_res} -m '{"id":"${op_id}", "status":"executing"}'
    Execute Command    curl ${file_transfer_url} --output firmware1
    Execute Command    mosquitto_pub -h ${PARENT_IP} -t ${topic_res} -m '{"id":"${op_id}", "status":"successful"}'

Validate Cumulocity operation status and MO
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}
    Cumulocity.Device Should Have Firmware    firmware1    version=1.0    url=${file_url}

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=False
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}

    # Child
    ${child_sn}=    Setup    skip_bootstrap=True
    Set Suite Variable    $CHILD_SN    ${child_sn}
