#PRECONDITION: 
#Device CH_DEV_CONF_MGMT is existing on tenant, if not
#use -v DeviceID:xxxxxxxxxxx in the command line to use your existing device

*** Settings ***
Library    SSHLibrary
Library    DateTime
Library    MQTTLibrary
Library    CryptoLibrary    variable_decryption=True
Library    RequestsLibrary
Library    REST    https://qaenvironment.eu-latest.cumulocity.com
Library    JSONLibrary
Library    Collections

*** Variables ***

${PARENT_IP}             192.168.1.110
${CHILD_IP}              192.168.1.200
${HTTP_PORT}             8000
${USERNAME}              pi
${PASSWORD}              crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=
${url_tedge}             qaenvironment.eu-latest.cumulocity.com
${user}                  systest_preparation
${pass}                  crypt:34mpoxueRYy/gDerrLeBThQ2wp9F+2cw50XaNyjiGUpK488+1fgEfE6drOEcR+qZQ6dcjIWETukbqLU= 
${config}                "files = [\n\t { path = '/home/pi/config1', type = 'config1' },\n ]\n"
${PARENT_NAME}           CH_DEV_CONF_MGMT
${CHILD}                
${topic_snap}            /commands/res/config_snapshot"
${topic_upd}             /commands/res/config_update"
${payl_notify}           '{"status": null,  "path": "", "type":"c8y-configuration-plugin", "reason": null}'
${payl_exec}             '{"status": "executing", "path": "/home/pi/config1", "type": "config1", "reason": null}'
${payl_succ}             '{"status": "successful", "path": "/home/pi/config1", "type": "config1", "reason": null}'


*** Test Cases ***
Create child device name
    Create Timestamp                                    #Timestamp is used for unique names
    Define Child Device name                            #Adding CD in front of the timestamp
Clean devices from the cloud
    Remove all managedObjects from cloud                #Removing all existing devices from the tenant 
Prerequisite Parent
    Parent Connection                                   #Creates ssh connection to the parent device  
    ${rc}=    Execute Command    sudo tedge disconnect c8y    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}  
    
    Delete child related content                        #Delete any previous created child related configuration files/folders on the parent device
    Check for child related content                     #Checks if folders that will trigger child device creation are existing
    Set external MQTT bind address                      #Setting external MQTT bind address which child will use for communication 
    Set external MQTT port                              #Setting external MQTT port which child will use for communication Default:1883
    
    ${rc}=    Execute Command    sudo tedge connect c8y    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0} 
    Restart Configuration plugin                        #Stop and Start c8y-configuration-plugin
    Close Connection                                    #Closes the connection to the parent device
Prerequisite Child
    Child device delete configuration files             #Delete any previous created child related configuration files/folders on the child device
Prerequisite Cloud
    GET Parent ID                                       #Get the Parent ID from the cloud
    GET Parent name                                     #Get the Parent name from the cloud
Child device bootstrapping
    Startup child device                                #Setting up/Bootstrapping of a child device
Get child credentials
    GET Child ID                                        #Get the Child ID from the cloud
    GET Child name                                      #Get the Child name from the cloud
    Validate child Name                                 #This is to check the existence of the bug: https://github.com/thin-edge/thin-edge.io/issues/1569
Snapshot from device
    Request snapshot from child device                  #Using the cloud command: "Get snapshot from device"
    Child device response on snapshot request           #Child device is sending 'executing' and 'successful' MQTT responses
    No response from child device on snapshot request   #Tests the failing of request after timeout of 10s  
Child device config update
    Send configuration to device                        #Using the cloud command: "Send configuration to device"
    Child device response on update request             #Child device is sending 'executing' and 'successful' MQTT responses
    No response from child device on config update      #Tests the failing of request after timeout of 10s  



*** Keywords ***
Create Timestamp
    ${timestamp}=    get current date    result_format=%H%M%S
    Set Suite Variable    ${timestamp}
Define Child Device name
    ${CHILD}=   Set Variable    CD${timestamp}
    Set Suite Variable    ${CHILD}
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
Check for child related content
    @{dir_cont}    List Directories In Directory    /etc/tedge/operations/c8y
    Should Be Empty   ${dir_cont}
    @{dir_cont}    List Directories In Directory    /etc/tedge/c8y
    Should Be Empty   ${dir_cont}
    @{dir_cont}    List Directories In Directory    /var/tedge
    Should Be Empty   ${dir_cont}
Delete child related content
    Execute Command    sudo rm -rf /etc/tedge/operations/c8y/CD*         #if folder exists, child device will be created
    Execute Command    sudo rm c8y-configuration-plugin.toml
    Execute Command    sudo rm -rf /etc/tedge/c8y/CD*                    #if folder exists, child device will be created
    Execute Command    sudo rm -rf /var/tedge/*
Remove all managedObjects from cloud
    Get all existing managedObjects
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    url=/inventory/managedObjects?fragmentType=c8y_IsDevice
    ${json_response}=    Set Variable    ${Get_Response.json()}  
    @{id}=    Get Value From Json    ${json_response}    $..id   
    ${man_Obj_id}    Get From List    ${id}    1
    # Set Suite Variable    ${man_Obj_id}
    FOR    ${element}    IN    @{id}
           ${delete}=    Run Keyword And Ignore Error   Delete existing managedObject
           Set Suite Variable    ${element}   
    END
GET Parent ID
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    identity/externalIds/c8y_Serial/${PARENT_NAME}   #c8y identity list --device ${DeviceID}
    ${json_response}=    Set Variable    ${Get_Response.json()}  
    @{id}=    Get Value From Json    ${json_response}    $..id   
    ${parent_id}    Get From List    ${id}    0
    Set Suite Variable    ${parent_id}
Get Child ID
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /inventory/managedObjects/${parent_id}    #Command: c8y inventory get --id ${parent_id}
    ${json_response}=    Set Variable    ${Get_Response.json()}  
    @{id}=    Get Value From Json    ${json_response}    $..id   
    ${child_id}    Get From List    ${id}    1
    Set Suite Variable    ${child_id}
Check parent child relationship
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /inventory/managedObjects/${parent_id}/childDevices/${child_id}    expected_status=200    #Command: c8y inventory children get --childType device --id ${parent_id} --child ${child_id}
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
Child device delete configuration files
    Child Connection
    Execute Command    sudo rm config1
    Execute Command    sudo rm c8y-configuration-plugin
    Close Connection
GET Parent name
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /identity/globalIds/${parent_id}/externalIds
    ${json_response}=    Set Variable    ${Get_Response.json()}
    @{pd_name}=    Get Value From Json    ${json_response}    $..externalId
    ${pardev_name}    Get From List    ${pd_name}    0
    Set Suite Variable    ${pardev_name}
GET Child name
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /inventory/managedObjects?fragmentType\=c8y_IsDevice
    ${json_response}=    Set Variable    ${Get_Response.json()}  
    @{name}=    Get Value From Json    ${json_response}    $..managedObject.name
    ${child_name}    Get From List    ${name}    0
    Set Suite Variable    ${child_name}
Validate child Name
    Should Be Equal    ${CHILD}     ${child_name}
Startup child device
    Child Connection
    ${rc}=    Execute Command    printf ${config} > c8y-configuration-plugin    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Write    curl -X PUT http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/c8y-configuration-plugin \\            #Folder will be created /var/tedge/file-transfer
    Write   --data-binary @- << EOF
    Write   files = [
    Write        { path = '/home/pi/config1', type = 'config1' },
    Write    ]
    Write  EOF 
    Execute Command    sudo apt-get install mosquitto-clients -y
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD}${topic_snap} -m ${payl_notify}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Close Connection
Request snapshot from child device
    ${json_snap}=    Set Variable    {"deviceId":"${child_id}","description":"Retrieve config1 configuration snapshot from device ${CHILD}","c8y_UploadConfigFile":{"type":"config1"}}
    Connect    ${PARENT_IP}
    @{messages}=    Subscribe    tedge/${CHILD}/commands/req/config_snapshot    qos=1    timeout=0   limit=0
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    POST On Session    API_Testing    /devicecontrol/operations    ${json_snap}    #expected_status=200
    @{listen}=    Listen    tedge/${CHILD}/commands/req/config_snapshot    timeout=20    limit=1
    Should Be Equal    @{listen}    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_snapshot/config1","path":"/home/pi/config1","type":"config1"}
    [Teardown]    Disconnect
    #CHECK OPERATION 
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /devicecontrol/operations
    ${json_response}=    Set Variable    ${Get_Response.json()}
    @{pd_name}=    Get Value From Json    ${json_response}    $..status
    ${first}    Get From List    ${pd_name}    0
    ${second}    Get From List    ${pd_name}    1
    ${third}    Get From List    ${pd_name}    2
    ${fourth}    Get From List    ${pd_name}    3
    Should Be Equal    ${first}    PENDING
Child device response on snapshot request    
    Child Connection
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD}${topic_snap} -m ${payl_exec}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    ${rc}=    Execute Command    curl -X PUT --data-binary @/home/pi/config1 http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_snapshot/config1    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Sleep    5s
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD}${topic_snap} -m ${payl_succ}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Sleep    2s
    Close Connection
    #CHECK OPERATION 
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /devicecontrol/operations
    ${json_response}=    Set Variable    ${Get_Response.json()}
    @{pd_name}=    Get Value From Json    ${json_response}    $..status
    ${first}    Get From List    ${pd_name}    0
    ${second}    Get From List    ${pd_name}    1
    ${third}    Get From List    ${pd_name}    2
    ${fourth}    Get From List    ${pd_name}    3
    Should Be Equal    ${first}    SUCCESSFUL
Send configuration to device
    ${json_conf}=    Set Variable    {"deviceId":"${child_id}","description":"Send configuration snapshot config1 of configuration type config1 to device ${CHILD}","c8y_DownloadConfigFile":{"url":"https://${url_tedge}/inventory/binaries/21315","type":"config1"}}
    Connect    ${PARENT_IP}   
    @{messages}=    Subscribe    tedge/${CHILD}/commands/req/config_update    qos=1    timeout=0   limit=0
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    POST On Session    API_Testing   /devicecontrol/operations    ${json_conf}
    @{listen}=    Listen    tedge/${CHILD}/commands/req/config_update    timeout=20    limit=1
    Should Be Equal    @{listen}    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_update/config1","path":"/home/pi/config1","type":"config1"}
    [Teardown]    Disconnect
    #CHECK OPERATION 
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /devicecontrol/operations
    ${json_response}=    Set Variable    ${Get_Response.json()}
    @{pd_name}=    Get Value From Json    ${json_response}    $..status
    ${first}    Get From List    ${pd_name}    0
    ${second}    Get From List    ${pd_name}    1
    ${third}    Get From List    ${pd_name}    2
    ${fourth}    Get From List    ${pd_name}    3
    Should Be Equal    ${second}    DELIVERED
Child device response on update request
    Child Connection
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD}${topic_upd} -m ${payl_exec}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    ${rc}=    Execute Command    curl http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_update/config1 --output config1    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    # Sleep    5s             #Enable if tests starts to fail
    ${rc}=    Execute Command    mosquitto_pub -h ${PARENT_IP} -t "tedge/${CHILD}${topic_upd} -m ${payl_succ}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    # Sleep    2s             #Enable if tests starts to fail
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /devicecontrol/operations
    ${json_response}=    Set Variable    ${Get_Response.json()}
    @{pd_name}=    Get Value From Json    ${json_response}    $..status
    ${first}    Get From List    ${pd_name}    0
    ${second}    Get From List    ${pd_name}    1
    ${third}    Get From List    ${pd_name}    2
    ${fourth}    Get From List    ${pd_name}    3
    Should Be Equal    ${first}    SUCCESSFUL
Delete child device
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    DELETE On Session    API_Testing     /inventory/managedObjects/${child_id}    #expected_status=204    #Command: c8y inventory delete --id ${child_id}
No response from child device on snapshot request
    ${json_snap}=    Set Variable    {"deviceId":"${child_id}","description":"Retrieve config1 configuration snapshot from device ${CHILD}","c8y_UploadConfigFile":{"type":"config1"}}
    Connect    ${PARENT_IP}
    @{messages}=    Subscribe    tedge/${CHILD}/commands/req/config_snapshot    qos=1    timeout=0   limit=0
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    POST On Session    API_Testing    /devicecontrol/operations    ${json_snap}    #expected_status=200
    @{listen}=    Listen    tedge/${CHILD}/commands/req/config_snapshot    timeout=20    limit=1
    Should Be Equal    @{listen}    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_snapshot/config1","path":"/home/pi/config1","type":"config1"}
    [Teardown]    Disconnect
    #CHECK TIMEOUT MESSAGE
    Sleep    11s
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /devicecontrol/operations
    ${json_response}=    Set Variable    ${Get_Response.json()}
    @{pd_name}=    Get Value From Json    ${json_response}    $..failureReason
    Should Contain    ${pd_name}    Timeout due to lack of response from child device: ${CHILD} for config type: config1

No response from child device on config update
    ${json_conf}=    Set Variable    {"deviceId":"${child_id}","description":"Send configuration snapshot config1 of configuration type config1 to device ${CHILD}","c8y_DownloadConfigFile":{"url":"https://${url_tedge}/inventory/binaries/21315","type":"config1"}}
    Connect    ${PARENT_IP}   
    @{messages}=    Subscribe    tedge/${CHILD}/commands/req/config_update    qos=1    timeout=0   limit=0
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    POST On Session    API_Testing   /devicecontrol/operations    ${json_conf}
    @{listen}=    Listen    tedge/${CHILD}/commands/req/config_update    timeout=20    limit=1
    Should Be Equal    @{listen}    {"url":"http://${PARENT_IP}:${HTTP_PORT}/tedge/file-transfer/${CHILD}/config_update/config1","path":"/home/pi/config1","type":"config1"}
    [Teardown]    Disconnect
    #CHECK OPERATION 
    Sleep    11s
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    /devicecontrol/operations
    ${json_response}=    Set Variable    ${Get_Response.json()}
    @{pd_name}=    Get Value From Json    ${json_response}    $..failureReason
    ${first}    Get From List    ${pd_name}    0
    Should Be Equal    ${first}    Timeout due to lack of response from child device: ${CHILD} for config type: config1

Delete all existing managedObjects
    ${rc}=    Execute Command       c8y devices list --includeAll | c8y devices delete --force    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
Get all existing managedObjects
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    GET On Session    API_Testing    url=/inventory/managedObjects?fragmentType=c8y_IsDevice
    ${json_response}=    Set Variable    ${Get_Response.json()}  
    @{id}=    Get Value From Json    ${json_response}    $..id   
    ${man_Obj_id}    Get From List    ${id}    1
    Set Suite Variable    ${man_Obj_id}
Delete existing managedObjects
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    DELETE On Session    API_Testing     /inventory/managedObjects/${man_Obj_id}    #expected_status=204    #Command: c8y inventory delete --id ${child_id}
Delete existing managedObject
    ${auth}=    Create List    ${user}    ${pass}
    Create Session    API_Testing    https://${url_tedge}    auth=${auth}
    ${Get_Response}=    DELETE On Session    API_Testing     /inventory/managedObjects/${element}
