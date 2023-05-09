*** Comments ***
#PRECONDITION:
#Device CH_DEV_CONF_MGMT is existing on tenant, if not
#use -v DeviceID:xxxxxxxxxxx in the command line to use your existing device


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             JSONLibrary
Library             Collections

Suite Setup         Custom Setup
Suite Teardown      Get Logs    name=${PARENT_SN}

Force Tags          theme:configuration    theme:childdevices


*** Variables ***
${PARENT_IP}
${HTTP_PORT}        8000

${config}           "files = [\n\t { path = '/home/pi/config1', type = 'config1' },\n ]\n"
${PARENT_SN}
${CHILD_SN}

${topic_snap}       /commands/res/config_snapshot"
${topic_upd}        /commands/res/config_update"
${payl_notify}      '{"status": null,    "path": "", "type":"c8y-configuration-plugin", "reason": null}'
${payl_exec}        '{"status": "executing", "path": "/home/pi/config1", "type": "config1", "reason": null}'
${payl_succ}        '{"status": "successful", "path": "/home/pi/config1", "type": "config1", "reason": null}'

${CHILD_CONFIG}=    SEPARATOR=\n
...                 files = [
...                 { path = '/home/pi/config1', type = 'config1' },
...                 ]


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
    Restart Configuration plugin    #Stop and Start c8y-configuration-plugin
    Cumulocity.Log Device Info

Prerequisite Child
    Child device delete configuration files    #Delete any previous created child related configuration files/folders on the child device

Child device bootstrapping
    Startup child device    #Setting up/Bootstrapping of a child device
    Validate child Name    #This is to check the existence of the bug: https://github.com/thin-edge/thin-edge.io/issues/1569

Snapshot from device
    Request snapshot from child device    #Using the cloud command: "Get snapshot from device"
    Child device response on snapshot request    #Child device is sending 'executing' and 'successful' MQTT responses
    No response from child device on snapshot request    #Tests the failing of request after timeout of 10s

Child device config update
    Send configuration to device    #Using the cloud command: "Send configuration to device"
    Child device response on update request    #Child device is sending 'executing' and 'successful' MQTT responses
    No response from child device on config update    #Tests the failing of request after timeout of 10s


*** Keywords ***
Set external MQTT bind address
    Set Device Context    ${PARENT_SN}
    Execute Command    sudo tedge config set mqtt.external.bind.address ${PARENT_IP}

Set external MQTT port
    Set Device Context    ${PARENT_SN}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883

Check for child related content
    Set Device Context    ${CHILD_SN}
    Directory Should Not Have Sub Directories    /etc/tedge/operations/c8y
    Directory Should Not Have Sub Directories    /etc/tedge/c8y
    Directory Should Not Have Sub Directories    /var/tedge

Delete child related content
    Execute Command    sudo rm -rf /etc/tedge/operations/c8y/TST*    #if folder exists, child device will be created
    Execute Command    sudo rm -f c8y-configuration-plugin.toml
    Execute Command    sudo rm -rf /etc/tedge/c8y/TST*    #if folder exists, child device will be created
    Execute Command    sudo rm -rf /var/tedge/*

Check parent child relationship
    Cumulocity.Set Device    ${PARENT_SN}
    Cumulocity.Device Should Have A Child Devices    ${CHILD_SN}

Reconnect c8y
    Execute Command    sudo tedge disconnect c8y
    Execute Command    sudo tedge connect c8y

Restart Configuration plugin
    Restart Service    c8y-configuration-plugin.service
    # Execute Command    sudo systemctl stop c8y-configuration-plugin.service
    # Execute Command    sudo systemctl start c8y-configuration-plugin.service

Child device delete configuration files
    Set Device Context    ${CHILD_SN}
    Execute Command    sudo rm -f config1
    Execute Command    sudo rm -f c8y-configuration-plugin

Validate child Name
    Device Should Exist    ${CHILD_SN}
    ${child_mo}=    Cumulocity.Device Should Have Fragments    name
    Should Be Equal    device_${PARENT_SN}    ${child_mo["owner"]}    # The parent is the owner of the child

Startup child device
    Sleep    5s    reason=The registration of child devices is flakey
    Set Device Context    ${CHILD_SN}
    Execute Command    printf ${config} > c8y-configuration-plugin

    Execute Command
    ...    curl -X PUT http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/c8y-configuration-plugin --data-binary "${CHILD_CONFIG}"

    Sleep    5s    reason=The registration of child devices is flakey

    Execute Command    sudo apt-get install mosquitto-clients -y
    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD_SN}${topic_snap} -m ${payl_notify} -q 1

Request snapshot from child device
    Cumulocity.Set Device    ${CHILD_SN}
    ${operation}=    Get Configuration    config1
    Set Suite Variable    $operation

    Set Device Context    ${PARENT_SN}
    @{listen}=    Should Have MQTT Messages    topic=tedge/${CHILD_SN}/commands/req/config_snapshot    date_from=-5s
    Should Be Equal
    ...    @{listen}
    ...    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/config_snapshot/config1","path":"/home/pi/config1","type":"config1"}

    #CHECK OPERATION
    Cumulocity.Operation Should Be PENDING    ${operation}

Child device response on snapshot request
    Set Device Context    ${CHILD_SN}
    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD_SN}${topic_snap} -m ${payl_exec}

    Execute Command
    ...    curl -X PUT --data-binary @/home/pi/config1 http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/config_snapshot/config1

    Sleep    5s
    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD_SN}${topic_snap} -m ${payl_succ}

    Sleep    2s

    #CHECK OPERATION
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}

Send configuration to device
    ${file_url}=    Create Inventory Binary    test-config.toml    config1    contents=Dummy config
    ${operation}=    Set Configuration
    ...    config1
    ...    ${file_url}
    ...    description=Send configuration snapshot config1 of configuration type config1 to device ${CHILD_SN}
    Set Suite Variable    $operation

    Set Device Context    ${PARENT_SN}
    @{listen}=    Should Have MQTT Messages    topic=tedge/${CHILD_SN}/commands/req/config_update    date_from=-5s
    Should Be Equal
    ...    @{listen}
    ...    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/config_update/config1","path":"/home/pi/config1","type":"config1"}

    #CHECK OPERATION
    Operation Should Be DELIVERED    ${operation}

Child device response on update request
    Set Device Context    ${CHILD_SN}
    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD_SN}${topic_upd} -m ${payl_exec}

    Execute Command
    ...    curl http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/config_update/config1 --output config1

    # Sleep    5s    #Enable if tests starts to fail
    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD_SN}${topic_upd} -m ${payl_succ}

    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}

No response from child device on snapshot request
    ${operation}=    Get Configuration    config1

    Set Device Context    ${PARENT_SN}
    @{listen}=    Should Have MQTT Messages    topic=tedge/${CHILD_SN}/commands/req/config_snapshot
    Should Be Equal
    ...    @{listen}
    ...    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/config_snapshot/config1","path":"/home/pi/config1","type":"config1"}

    #CHECK TIMEOUT MESSAGE
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=Timeout due to lack of response from child device: ${CHILD_SN} for config type: config1
    ...    timeout=60

No response from child device on config update
    ${file_url}=    Create Inventory Binary    test-config.toml    config1    contents=Dummy config
    ${operation}=    Set Configuration    config1    ${file_url}

    Set Device Context    ${PARENT_SN}
    @{listen}=    Should Have MQTT Messages    topic=tedge/${CHILD_SN}/commands/req/config_update
    Should Be Equal
    ...    @{listen}
    ...    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD_SN}/config_update/config1","path":"/home/pi/config1","type":"config1"}

    #CHECK OPERATION
    Cumulocity.Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=Timeout due to lack of response from child device: ${CHILD_SN} for config type: config1
    ...    timeout=60

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=False
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}

    # Child
    ${child_sn}=    Setup    skip_bootstrap=True
    Set Suite Variable    $CHILD_SN    ${child_sn}
