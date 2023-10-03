*** Comments ***
#Command to execute:    robot -d \results --timestampoutputs --log data_path_config.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io/tests/RobotFramework/customizing/data_path_config.robot


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             JSONLibrary
Library             String
Library             DebugLibrary

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Force Tags          theme:cli    theme:configuration


*** Test Cases ***
Validate updated data path used by tedge-agent
    Restart Service    tedge-agent
    Directory Should Exist    /var/test/file-transfer

Validate updated data path used by c8y-firmware-plugin
    Restart Service    c8y-firmware-plugin
    Directory Should Exist    /var/test/firmware
    Directory Should Exist    /var/test/cache
    Bootstrap child device with firmware operation support
    ${firmware_url}=    Upload firmware binary to Cumulocity
    ${date_from}=    Get Unix Timestamp
    Create c8y_Firmware operation    ${firmware_url}
    ${op_id}    ${cache_key}=    Validate tedge firmware update request sent    ${date_from}
    File Should Exist    /var/test/firmware/${op_id}
    File Should Exist    /var/test/cache/${cache_key}


*** Keywords ***
Custom Setup
    ${PARENT_SN}=    Setup
    Set Suite Variable    $PARENT_SN
    Set Suite Variable    $CHILD_SN    ${PARENT_SN}_child
    Execute Command    sudo mkdir /var/test
    Execute Command    sudo chown tedge:tedge /var/test
    Execute Command    sudo tedge config set data.path /var/test

Custom Teardown
    Execute Command    sudo rm -rf /var/test
    Execute Command    sudo tedge config unset data.path
    Get Logs

Bootstrap child device with firmware operation support
    Execute Command    mkdir -p /etc/tedge/operations/c8y/${CHILD_SN}
    Sleep    3s
    Execute Command    touch /etc/tedge/operations/c8y/${CHILD_SN}/c8y_Firmware
    Sleep    3s
    Cumulocity.Device Should Exist    ${CHILD_SN}

Upload firmware binary to Cumulocity
    ${file_url}=    Cumulocity.Create Inventory Binary    firmware1.txt    firmware1    contents="firmware1"
    RETURN    ${file_url}

Create c8y_Firmware operation
    [Arguments]    ${firmware_url}
    ${operation}=    Cumulocity.Install Firmware    firmware1    1.0    ${firmware_url}
    Set Suite Variable    $operation
    Cumulocity.Operation Should Be DELIVERED    ${operation}

Validate tedge firmware update request sent
    [Arguments]    ${date_from}
    ${listen}=    ThinEdgeIO.Should Have MQTT Messages
    ...    topic=tedge/${CHILD_SN}/commands/req/firmware_update
    ...    date_from=${date_from}
    ${message}=    JSONLibrary.Convert String To Json    ${listen[0]}
    ${op_id}=    Set Variable    ${message["id"]}
    ${cache_id}=    Get file id from tedge url    ${message["url"]}
    RETURN    ${op_id}    ${cache_id}

Get file id from tedge url
    [Arguments]    ${firmware_url}
    ${url_split}=    String.Split String From Right    ${firmware_url}    separator=/    max_split=1
    RETURN    ${url_split[1]}
