*** Settings ***
# Library    Dialogs
Library    SSHLibrary
Library    DateTime
Library    MQTTLibrary
Library    CryptoLibrary    variable_decryption=True
Library    RequestsLibrary
Library    REST    https://qaenvironment.eu-latest.cumulocity.com
Library    JSONLibrary
Library    Collections

*** Variables ***

${PARENT_IP}             192.168.1.120
${CHILD_IP}              192.168.1.200
${HTTP_PORT}             8000
${USERNAME}              pi
${PASSWORD}              crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=
${url_tedge}             qaenvironment.eu-latest.cumulocity.com
${user}                  systest_preparation
${pass}                  crypt:OBusFTXwz00ge67sjHgP8kkH0jLrso7rp6Bp4sHKhwULvkr/nd8WHGezY37/fmMBeNvn+Xxk558glujxb5Pj
${config}                "files = [\n\t { path = '/home/pi/config1', type = 'config1' },\n ]\n"

${DeviceID}              CH_DEV_CONF_MGMT
${CHILD}                 sensor1
${topic_snap}           "tedge/${CHILD}/commands/res/config_snapshot"
${topic_upd}            "tedge/${CHILD}/commands/res/config_update"
${topic_restart}        "c8y/s/us/${CHILD}"
${payl_restart}         "114,c8y_UploadConfigFile,c8y_DownloadConfigFile"
${payl_notify}          '{"status": null,  "path": "", "type":"c8y-configuration-plugin", "reason": null}'
${payl_exec}            '{"status": "executing", "path": "/home/pi/config1", "type": "config1", "reason": null}'
${payl_succ}            '{"status": "successful", "path": "/home/pi/config1", "type": "config1", "reason": null}'


*** Test Cases ***
Prerequisite Parent
    Parent Connection
    Delete child related content                    #Delete any previous created child related configuration files/folders on the parent device
    Set external MQTT bind address                  #Setting external MQTT bind address which child will use for communication 
    Set external MQTT port                          #Setting external MQTT port which child will use for communication Default:1883
    Reconnect c8y                                   #Disconnect and Connect to c8y
    Restart Configuration plugin                    #Stop and Start c8y-configuration-plugin
    Close Connection
Prerequisite Child
    Child device delete configuration files         #Delete any previous created child related configuration files/folders on the child device
Prerequisite Cloud
    GET Parent ID                                   #Get the Parent ID from the cloud
    GET Parent name                                 #Get the Parent name from the cloud
    GET Child ID                                    #Get the Child ID from the cloud
    GET Child name                                  #Get the Child name from the cloud
    # Validate child Name                             #This is to check the existence of the bug: https://github.com/thin-edge/thin-edge.io/issues/1569
Child device bootstrapping
    Startup child device                            #Setting up/Bootstraping of a child device
Snapshot from device
    Request snapshot from child device              #Using the cloud command Get snapshot from device
    Child device response on snapshot request       #Child device is sending 'executing' and 'successful' MQTT responses
Child device config update
    Send configuration to device                    #Using the cloud command Send configuration to device
    Child device get configuration file             #Child device is sending 'executing' and 'successful' MQTT responses


*** Keywords ***
Parent Connection
    Open Connection     ${PARENT_IP}
    Login               ${USERNAME}    ${PASSWORD}
Child Connection
    Open Connection     ${CHILD_IP}
    Login               ${USERNAME}    ${PASSWORD}
Set external MQTT bind address
    ${rc}=    Execute Command    sudo tedge config set mqtt.external.bind_address ${PARENT_IP}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
Set external MQTT port
    ${rc}=    Execute Command    sudo tedge config set mqtt.external.port 1883    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

Delete child related content
    Execute Command    sudo rm -rf /etc/tedge/operations/c8y/CD*
    Execute Command    sudo rm c8y-configuration-plugin.toml
    Execute Command    sudo rm -rf /etc/tedge/c8y/CD*
    Execute Command    sudo rm -rf /var/tedge/*

GET Parent ID
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /inventory/managedObjects?fragmentType\=c8y_IsDevice
    ${json_response}=    Set Variable    ${Get_Response.json()}  
    @{id}=    Get Value From Json    ${json_response}    $..id   
    ${parent_id}    Get From List    ${id}    0
    Set Suite Variable    ${parent_id}
GET Parent name
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /identity/globalIds/${parent_id}/externalIds
    ${json_response}=    Set Variable    ${Get_Response.json()}
    @{pd_name}=    Get Value From Json    ${json_response}    $..externalId
    ${pardev_name}    Get From List    ${pd_name}    0
    Set Suite Variable    ${pardev_name}
GET Child ID
    ${auth}=    Create List     ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /inventory/managedObjects?fragmentType\=c8y_IsDevice
    ${json_response}=    Set Variable    ${Get_Response.json()}  
    @{id}=    Get Value From Json    ${json_response}    $..managedObject.id
    ${child_id}    Get From List    ${id}    0
    Set Suite Variable    ${child_id}
GET Child name
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /inventory/managedObjects?fragmentType\=c8y_IsDevice
    ${json_response}=    Set Variable    ${Get_Response.json()}  
    @{name}=    Get Value From Json    ${json_response}    $..managedObject.name
    ${child_name}    Get From List    ${name}    0
    Set Suite Variable    ${child_name}
Validate child name
    ${validate}=    ${child_name}
    Should Be Equal    ${validate}    ${CHILD} 
Reconnect c8y
    ${rc}=    Execute Command    sudo tedge disconnect c8y    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}   
    ${rc}=    Execute Command    sudo tedge connect c8y    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0} 
Restart Configuration plugin
    ${rc}=    Execute Command    sudo systemctl stop c8y-configuration-plugin.service    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0} 
    ${rc}=    Execute Command    sudo systemctl start c8y-configuration-plugin.service    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0} 
Startup child device
    Child Connection
    ${rc}=    Execute Command    printf ${config} > c8y-configuration-plugin    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Write    curl -X PUT http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/c8y-configuration-plugin \\
    Write   --data-binary @- << EOF
    Write   files = [
    Write        { path = '/home/pi/config1', type = 'config1' },
    Write    ]
    Write  EOF 
    Execute Command    sudo apt-get install mosquitto mosquitto-clients -y
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t ${topic_snap} -m ${payl_notify}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Close Connection
Request snapshot from child device  
    ${json_snap}=    Set Variable    {"deviceId":"${child_id}","description":"Retrieve config1 configuration snapshot from device ${CHILD}","c8y_UploadConfigFile":{"type":"config1"}}
    Connect    ${PARENT_IP}   
    @{messages}=    Subscribe    tedge/${CHILD}/commands/req/config_snapshot    qos=1    timeout=0   limit=0
    Set Client Authentication    basic    ${user}    ${pass} 
    Rest.POST    /devicecontrol/operations    ${json_snap}
    @{listen}=    Listen    tedge/${CHILD}/commands/req/config_snapshot    timeout=20    limit=1
    Should Be Equal    @{listen}    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_snapshot/config1","path":"/home/pi/config1","type":"config1"}
    [Teardown]    Disconnect
Send configuration to device
    ${json_conf}=    Set Variable    {"deviceId":"${child_id}","description":"Send configuration snapshot config1 of configuration type config1 to device ${CHILD}","c8y_DownloadConfigFile":{"url":"https://${url_tedge}/inventory/binaries/21315","type":"config1"}}
    Connect    ${PARENT_IP}   
    @{messages}=    Subscribe    tedge/${CHILD}/commands/req/config_update    qos=1    timeout=0   limit=0
    Set Client Authentication    basic    ${user}    ${pass} 
    Rest.POST    /devicecontrol/operations    ${json_conf}
    @{listen}=    Listen    tedge/${CHILD}/commands/req/config_update    timeout=20    limit=1
    Should Be Equal    @{listen}    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_update/config1","path":"/home/pi/config1","type":"config1"}
    [Teardown]    Disconnect
Child device delete configuration files
    Child Connection
    Execute Command    sudo rm config1
    Execute Command    sudo rm c8y-configuration-plugin
    Close Connection

Child device response on snapshot request    
    Child Connection
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t ${topic_snap} -m ${payl_exec}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    ${rc}=    Execute Command    curl -X PUT --data-binary @/home/pi/config1 http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_snapshot/config1    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Sleep    5s
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t ${topic_snap} -m ${payl_succ}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Sleep    2s
    Close Connection

Child device get configuration file
    Child Connection
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t ${topic_upd} -m ${payl_exec}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    ${rc}=    Execute Command    curl http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_update/config1 --output config1    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Sleep    5s
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t ${topic_upd} -m ${payl_succ}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Sleep    2s
