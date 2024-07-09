*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             OperatingSystem

Suite Setup         Suite Setup
Suite Teardown      Get Logs    name=${PARENT_SN}
Test Setup          Test Setup

Test Tags           theme:configuration    theme:childdevices


*** Variables ***
${PARENT_SN}    ${EMPTY}
${CHILD_SN}     ${EMPTY}


*** Test Cases ***    DEVICE    EXTERNALID    CONFIG_TYPE    DEVICE_FILE    FILE    PERMISSION    OWNERSHIP
#
# Set configuration
#
Set Configuration when file does not exist
    [Tags]    \#2318
    [Template]    Set Configuration from Device
    [Documentation]    If the configuration file does not exist, it should be created, with owner and permissions
    ...    specified in `tedge-configuration-plugin.toml` file.
    Text file (Main Device)    ${PARENT_SN}    ${PARENT_SN}    CONFIG1    /etc/config1.json    ${CURDIR}/config1-version2.json    640    tedge:tedge    delete_file_before=${true}
    Binary file (Main Device)    ${PARENT_SN}    ${PARENT_SN}    CONFIG1_BINARY    /etc/binary-config1.tar.gz    ${CURDIR}/binary-config1.tar.gz    640    tedge:tedge    delete_file_before=${true}
    Text file (Child Device)    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}    CONFIG1    /etc/config1.json    ${CURDIR}/config1-version2.json    640    tedge:tedge    delete_file_before=${true}
    Binary file (Child Device)    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}    CONFIG1_BINARY    /etc/binary-config1.tar.gz    ${CURDIR}/binary-config1.tar.gz    640    tedge:tedge    delete_file_before=${true}

Set Configuration when file exists
    [Tags]    \#2972
    [Template]    Set Configuration from Device
    [Documentation]    If the configuration file already exists, it should be overwritten, but owner and permissions
    ...    should remain unchanged.
    Text file (Main Device)    ${PARENT_SN}    ${PARENT_SN}    CONFIG1    /etc/config1.json    ${CURDIR}/config1-version2.json    664    root:root    delete_file_before=${false}
    Binary file (Main Device)    ${PARENT_SN}    ${PARENT_SN}    CONFIG1_BINARY    /etc/binary-config1.tar.gz    ${CURDIR}/binary-config1.tar.gz    664    root:root    delete_file_before=${false}
    Text file (Child Device)    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}    CONFIG1    /etc/config1.json    ${CURDIR}/config1-version2.json    664    root:root    delete_file_before=${false}
    Binary file (Child Device)    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}    CONFIG1_BINARY    /etc/binary-config1.tar.gz    ${CURDIR}/binary-config1.tar.gz    664    root:root    delete_file_before=${false}

Set configuration with broken url
    [Template]    Set Configuration from URL
    Main Device    ${PARENT_SN}    ${PARENT_SN}    CONFIG1    /etc/config1.json    invalid://hellö.zip
    Child Device    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}    CONFIG1    /etc/config1.json    invalid://hellö.zip

#
# Get configuration
#

Get Configuration from Main Device
    [Template]    Get Configuration from Device
    Text file    device=${PARENT_SN}    external_id=${PARENT_SN}    config_type=CONFIG1    device_file=/etc/config1.json
    Binary file    device=${PARENT_SN}    external_id=${PARENT_SN}    config_type=CONFIG1_BINARY    device_file=/etc/binary-config1.tar.gz

Get Configuration from Child Device
    [Tags]    \#2318
    [Template]    Get Configuration from Device
    Text file    device=${CHILD_SN}    external_id=${PARENT_SN}:device:${CHILD_SN}    config_type=CONFIG1    device_file=/etc/config1.json
    Binary file    device=${CHILD_SN}    external_id=${PARENT_SN}:device:${CHILD_SN}    config_type=CONFIG1_BINARY    device_file=/etc/binary-config1.tar.gz

Get Unknown Configuration Type From Device
    [Template]    Get Unknown Configuration Type From Device
    Main Device    ${PARENT_SN}    unknown_type
    Child Device    ${PARENT_SN}:device:${CHILD_SN}    unknown_type

Get non existent configuration file From Device
    [Template]    Get non existent configuration file From Device
    Main Device    ${PARENT_SN}    ${PARENT_SN}    CONFIG1    /etc/config1.json
    Child Device    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}    CONFIG1    /etc/config1.json

#
# Configuration Types
#

Update configuration plugin config via cloud
    [Template]    Update configuration plugin config via cloud
    Main Device    ${PARENT_SN}
    Child Device    ${PARENT_SN}:device:${CHILD_SN}

Modify configuration plugin config via local filesystem modify inplace
    [Template]    Modify configuration plugin config via local filesystem modify inplace
    Main Device    ${PARENT_SN}    ${PARENT_SN}
    Child Device    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}

Modify configuration plugin config via local filesystem overwrite
    [Template]    Modify configuration plugin config via local filesystem overwrite
    Main Device    ${PARENT_SN}    ${PARENT_SN}
    Child Device    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}

Update configuration plugin config via local filesystem copy
    [Template]    Update configuration plugin config via local filesystem copy
    Main Device    ${PARENT_SN}    ${PARENT_SN}
    Child Device    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}

Update configuration plugin config via local filesystem move (different directory)
    [Template]    Update configuration plugin config via local filesystem move (different directory)
    Main Device    ${PARENT_SN}    ${PARENT_SN}
    Child Device    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}

Update configuration plugin config via local filesystem move (same directory)
    [Template]    Update configuration plugin config via local filesystem move (same directory)
    Main Device    ${PARENT_SN}    ${PARENT_SN}
    Child Device    ${CHILD_SN}    ${PARENT_SN}:device:${CHILD_SN}

Manual config_snapshot operation request
    Set Device Context    ${PARENT_SN}
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/config_snapshot/local-1111
    ...    payload={"status":"init","tedgeUrl":"http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/config_snapshot/local-1111","type":"tedge-configuration-plugin"}
    ...    expected_status=successful
    ...    c8y_fragment=c8y_UploadConfigFile

Trigger config_snapshot operation from another operation
    Set Device Context    ${PARENT_SN}
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/sub_config_snapshot/sub-1111
    ...    payload={"status":"init","tedgeUrl":"http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/sub_config_snapshot/sub-1111","type":"tedge-configuration-plugin"}
    ...    expected_status=successful
    ...    c8y_fragment=c8y_UploadConfigFile
    ${snapshot}    Execute Command    curl http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/sub_config_snapshot/sub-1111
    ${config}      Get File    ${CURDIR}/tedge-configuration-plugin.toml
    Should Be Equal    ${snapshot}     ${config}

Trigger custom config_snapshot operation
    [Teardown]    Restore config operations
    Set Device Context    ${PARENT_SN}
    Customize config operations
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/config_snapshot/custom-1111
    ...    payload={"status":"init","tedgeUrl":"http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/config_snapshot/custom-1111","type":"tedge-configuration-plugin"}
    ...    expected_status=successful
    ...    c8y_fragment=c8y_UploadConfigFile
    ${snapshot}    Execute Command    curl http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/config_snapshot/custom-1111
    ${config}      Get File    ${CURDIR}/tedge-configuration-plugin.toml
    Should Be Equal    ${snapshot}     ${config}

Config_snapshot operation request with the tedgeUrl created by agent
    Set Device Context    ${PARENT_SN}
    ${timestamp}=    Get Unix Timestamp
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/config_snapshot/local-3333
    ...    payload={"status":"init","type":"tedge-configuration-plugin"}
    ...    expected_status=successful
    ...    c8y_fragment=c8y_UploadConfigFile

    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/config_snapshot/local-3333
    ...    message_contains=http://${PARENT_IP}:8000/tedge/file-transfer/main/config_snapshot/tedge-configuration-plugin-local-3333
    ...    date_from=${timestamp}

    ${output}=    Execute Command    curl -sSLf "http://${PARENT_IP}:8000/tedge/file-transfer/main/config_snapshot/tedge-configuration-plugin-local-3333"    strip=${True}
    Should Match Regexp    ${output}    pattern=files\\s*=\\s*\\[.*\\]    flags=DOTALL
    
Manual config_update operation request
    Set Device Context    ${PARENT_SN}
    # Don't worry about the command failing, that is expected since the tedgeUrl path does not exist
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/config_update/local-2222
    ...    payload={"status":"init","tedgeUrl":"http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/config_update/local-2222","remoteUrl":"","type":"tedge-configuration-plugin"}
    ...    expected_status=failed
    ...    c8y_fragment=c8y_DownloadConfigFile

Trigger config_update operation from another workflow
    Set Device Context    ${PARENT_SN}

    Execute Command     curl -X PUT --data-binary 'new content for CONFIG1' "http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/sub_config_update/sub-2222"
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/sub_config_update/sub-2222
    ...    payload={"status":"init","tedgeUrl":"http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/sub_config_update/sub-2222","remoteUrl":"","type":"CONFIG1"}
    ...    expected_status=successful
    ...    c8y_fragment=c8y_DownloadConfigFile

    ${update}           Execute Command    cat /etc/config1.json
    Should Be Equal     ${update}     new content for CONFIG1

Trigger custom config_update operation
    [Teardown]    Restore config operations
    Set Device Context    ${PARENT_SN}
    Customize config operations

    Execute Command     curl -X PUT --data-binary 'updated config' "http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/config_update/custom-2222"
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/config_update/custom-2222
    ...    payload={"status":"init","tedgeUrl":"http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/config_update/custom-2222","remoteUrl":"","type":"/tmp/config_update_target"}
    ...    expected_status=successful
    ...    c8y_fragment=c8y_DownloadConfigFile

    ${update}           Execute Command    cat /tmp/config_update_target
    Should Be Equal     ${update}     updated config

Config update request not processed when operation is disabled for tedge-agent
    [Teardown]    Enable config update capability of tedge-agent
    Set Device Context    ${PARENT_SN}
    Disable config update capability of tedge-agent
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/config_update/local-2222
    ...    payload={"status":"init","tedgeUrl":"http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/config_update/local-2222","remoteUrl":"","type":"tedge-configuration-plugin"}
    ...    expected_status=init
    ...    c8y_fragment=c8y_DownloadConfigFile

Config snapshot request not processed when operation is disabled for tedge-agent
    [Teardown]    Enable config snapshot capability of tedge-agent
    Set Device Context    ${PARENT_SN}
    Disable config snapshot capability of tedge-agent
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/config_snapshot/local-1111
    ...    payload={"status":"init","tedgeUrl":"http://${PARENT_IP}:8000/tedge/file-transfer/${PARENT_SN}/config_snapshot/local-1111","type":"tedge-configuration-plugin"}
    ...    expected_status=init
    ...    c8y_fragment=c8y_UploadConfigFile

Default plugin configuration
    Set Device Context    ${PARENT_SN}

    # Remove the existing plugin configuration
    Execute Command    rm /etc/tedge/plugins/tedge-configuration-plugin.toml

    # Agent restart should recreate the default plugin configuration
    Stop Service    tedge-agent
    ${timestamp}=        Get Unix Timestamp
    Start Service    tedge-agent
    Service Should Be Running    tedge-agent
    Should Have MQTT Messages    c8y/s/us    message_contains=119,    date_from=${timestamp}

    Cumulocity.Set Device    ${PARENT_SN}
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    tedge.toml
    ...    tedge-log-plugin
    

*** Keywords ***
Set Configuration from Device
    [Arguments]
    ...    ${test_desc}
    ...    ${device}
    ...    ${external_id}
    ...    ${config_type}
    ...    ${device_file}
    ...    ${file}
    ...    ${permission}
    ...    ${ownership}
    ...    ${delete_file_before}=${true}

    IF    ${delete_file_before}
        ThinEdgeIO.Set Device Context    ${device}
        Execute Command    rm -f ${device_file}
    END

    Cumulocity.Set Device    ${external_id}
    ${config_url}=    Cumulocity.Create Inventory Binary    temp_file    ${config_type}    file=${file}
    ${operation}=    Cumulocity.Set Configuration    ${config_type}    url=${config_url}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ThinEdgeIO.Set Device Context    ${device}
    File Checksum Should Be Equal    ${device_file}    ${file}
    Path Should Have Permissions    ${device_file}    ${permission}    ${ownership}

Set Configuration from URL
    [Arguments]    ${test_desc}    ${device}    ${external_id}    ${config_type}    ${device_file}    ${config_url}

    ThinEdgeIO.Set Device Context    ${device}
    ${hash_before}=    Execute Command    md5sum ${device_file}
    ${stat_before}=    Execute Command    stat ${device_file}

    Cumulocity.Set Device    ${external_id}
    ${operation}=    Cumulocity.Set Configuration    ${config_type}    url=${config_url}
    ${operation}=    Operation Should Be FAILED    ${operation}    timeout=120

    ${hash_after}=    Execute Command    md5sum ${device_file}
    ${stat_after}=    Execute Command    stat ${device_file}
    Should Be Equal    ${hash_before}    ${hash_after}
    Should Be Equal    ${stat_before}    ${stat_after}

Get Unknown Configuration Type From Device
    [Arguments]    ${test_desc}    ${external_id}    ${config_type}
    Cumulocity.Set Device    ${external_id}
    ${operation}=    Cumulocity.Get Configuration    ${config_type}
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*requested config_type "${config_type}" is not defined in the plugin configuration file.*

Get non existent configuration file From Device
    [Arguments]    ${test_desc}    ${device}    ${external_id}    ${config_type}    ${device_file}
    ThinEdgeIO.Set Device Context    ${device}
    ThinEdgeIO.Execute Command    rm -f ${device_file}
    Cumulocity.Set Device    ${external_id}
    ${operation}=    Cumulocity.Get Configuration    ${config_type}
    Operation Should Be FAILED    ${operation}    failure_reason=.*No such file or directory.*

Get Configuration from Device
    [Arguments]    ${test_name}    ${device}    ${external_id}    ${config_type}    ${device_file}
    Cumulocity.Set Device    ${external_id}
    ${operation}=    Cumulocity.Get Configuration    ${config_type}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ThinEdgeIO.Set Device Context    ${device}
    ${expected_checksum}=    Execute Command    md5sum '${device_file}' | cut -d' ' -f1    strip=${True}
    ${events}=    Cumulocity.Device Should Have Event/s
    ...    minimum=1
    ...    maximum=1
    ...    type=${config_type}
    ...    with_attachment=${True}
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${events[0]["id"]}
    ...    expected_md5=${expected_checksum}

    ${event}=    Cumulocity.Event Attachment Should Have File Info    ${events[0]["id"]}    name=^${external_id}_[\\w\\W]+-c8y-mapper-\\d+$

    RETURN    ${contents}

#
# Configuration Types
#

Update configuration plugin config via cloud
    [Arguments]    ${test_desc}    ${external_id}
    Cumulocity.Set Device    ${external_id}
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    CONFIG1_BINARY
    ${config_url}=    Cumulocity.Create Inventory Binary
    ...    tedge-configuration-plugin
    ...    tedge-configuration-plugin
    ...    file=${CURDIR}/tedge-configuration-plugin-updated.toml
    ${operation}=    Cumulocity.Set Configuration    tedge-configuration-plugin    url=${config_url}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    Config@2.0.0

Modify configuration plugin config via local filesystem modify inplace
    [Arguments]    ${test_desc}    ${device}    ${external_id}
    Cumulocity.Set Device    ${external_id}
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    CONFIG1_BINARY
    ThinEdgeIO.Set Device Context    ${device}
    ThinEdgeIO.Execute Command    sed -i 's/CONFIG1/CONFIG3/g' /etc/tedge/plugins/tedge-configuration-plugin.toml
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG3
    ...    CONFIG3_BINARY
    ${operation}=    Cumulocity.Get Configuration    CONFIG3
    Operation Should Be SUCCESSFUL    ${operation}

Modify configuration plugin config via local filesystem overwrite
    [Arguments]    ${test_desc}    ${device}    ${external_id}
    ThinEdgeIO.Set Device Context    ${device}
    Cumulocity.Set Device    ${external_id}
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    CONFIG1_BINARY
    ${NEW_CONFIG}=    ThinEdgeIO.Execute Command
    ...    sed 's/CONFIG1/CONFIG3/g' /etc/tedge/plugins/tedge-configuration-plugin.toml
    ThinEdgeIO.Execute Command    echo "${NEW_CONFIG}" > /etc/tedge/plugins/tedge-configuration-plugin.toml
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG3
    ...    CONFIG3_BINARY
    ${operation}=    Cumulocity.Get Configuration    CONFIG3
    Operation Should Be SUCCESSFUL    ${operation}

Update configuration plugin config via local filesystem copy
    [Arguments]    ${test_desc}    ${device}    ${external_id}
    ThinEdgeIO.Set Device Context    ${device}
    Cumulocity.Set Device    ${external_id}
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    CONFIG1_BINARY
    Transfer To Device    ${CURDIR}/tedge-configuration-plugin-updated.toml    /etc/tedge/plugins/
    Execute Command
    ...    cp /etc/tedge/plugins/tedge-configuration-plugin-updated.toml /etc/tedge/plugins/tedge-configuration-plugin.toml
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    Config@2.0.0
    ${operation}=    Cumulocity.Get Configuration    Config@2.0.0
    Operation Should Be SUCCESSFUL    ${operation}

Update configuration plugin config via local filesystem move (different directory)
    [Arguments]    ${test_desc}    ${device}    ${external_id}
    ThinEdgeIO.Set Device Context    ${device}
    Cumulocity.Set Device    ${external_id}
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    CONFIG1_BINARY
    Transfer To Device    ${CURDIR}/tedge-configuration-plugin-updated.toml    /etc/
    Execute Command
    ...    mv /etc/tedge-configuration-plugin-updated.toml /etc/tedge/plugins/tedge-configuration-plugin.toml
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    Config@2.0.0
    ${operation}=    Cumulocity.Get Configuration    Config@2.0.0
    Operation Should Be SUCCESSFUL    ${operation}

Update configuration plugin config via local filesystem move (same directory)
    [Arguments]    ${test_desc}    ${device}    ${external_id}
    ThinEdgeIO.Set Device Context    ${device}
    Cumulocity.Set Device    ${external_id}
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    CONFIG1_BINARY
    Transfer To Device    ${CURDIR}/tedge-configuration-plugin-updated.toml    /etc/tedge/plugins/
    Execute Command
    ...    mv /etc/tedge/plugins/tedge-configuration-plugin-updated.toml /etc/tedge/plugins/tedge-configuration-plugin.toml
    Cumulocity.Should Support Configurations
    ...    tedge-configuration-plugin
    ...    /etc/tedge/tedge.toml
    ...    system.toml
    ...    CONFIG1
    ...    Config@2.0.0
    ${operation}=    Cumulocity.Get Configuration    Config@2.0.0
    Operation Should Be SUCCESSFUL    ${operation}

Customize config operations
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom_config_snapshot.toml    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom_config_update.toml    /etc/tedge/operations/
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Restore config operations
    Execute Command    rm -f /etc/tedge/operations/custom_config_snapshot.toml
    Execute Command    rm -f /etc/tedge/operations/custom_config_update.toml
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

#
# Setup
#

Suite Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=${False}
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}
    Execute Command    sudo tedge config set mqtt.external.bind.address ${parent_ip}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883
    Execute Command    sudo tedge config set c8y.proxy.bind.address ${parent_ip}
    Execute Command    sudo tedge config set c8y.proxy.client.host ${parent_ip}
    Execute Command    sudo tedge config set http.client.host ${parent_ip}
    Restart Service    tedge-agent

    ThinEdgeIO.Disconnect Then Connect Mapper    c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

    # Child
    Setup Child Device    parent_ip=${parent_ip}    install_package=tedge-agent

Setup Child Device
    [Arguments]    ${parent_ip}    ${install_package}
    ${child_sn}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $CHILD_SN    ${child_sn}

    Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb packages/${install_package}*.deb

    Execute Command    sudo tedge config set mqtt.client.host ${parent_ip}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set http.client.host ${parent_ip}
    Execute Command    sudo tedge config set c8y.proxy.client.host ${parent_ip}
    Execute Command    sudo tedge config set mqtt.topic_root te
    Execute Command    sudo tedge config set mqtt.device_topic_id device/${child_sn}//

    Enable Service    ${install_package}
    Start Service    ${install_package}

    Copy Configuration Files    ${child_sn}

    RETURN    ${child_sn}

Test Setup
    Customize Operation Workflows    ${PARENT_SN}
    Customize Operation Workflows    ${CHILD_SN}
    Copy Configuration Files    ${PARENT_SN}
    Copy Configuration Files    ${CHILD_SN}

Copy Configuration Files
    [Arguments]    ${device}
    ThinEdgeIO.Set Device Context    ${device}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-configuration-plugin.toml    /etc/tedge/plugins/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config1.json    /etc/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config2.json    /etc/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/binary-config1.tar.gz    /etc/

    # make sure initial files have the same permissions on systems with different umasks
    Execute Command                  chmod 664 /etc/config1.json /etc/config2.json /etc/binary-config1.tar.gz

    # on a child device, user with uid 1000 doesn't exist, so make sure files we're testing on have a well defined user
    Execute Command    chown root:root /etc/tedge/plugins/tedge-configuration-plugin.toml /etc/config1.json /etc/binary-config1.tar.gz
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

Customize Operation Workflows
    [Arguments]    ${device}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/sub_config_snapshot.toml    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/sub_config_update.toml    /etc/tedge/operations/
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Publish and Verify Local Command
    [Arguments]    ${topic}    ${payload}    ${expected_status}=successful    ${c8y_fragment}=
    Execute Command    tedge mqtt pub --retain '${topic}' '${payload}'
    ${messages}=    Should Have MQTT Messages
    ...    ${topic}
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="status":"${expected_status}"

    Sleep    5s    reason=Given mapper a chance to react, if it does not react with 5 seconds it never will
    ${retained_message}=    Execute Command
    ...    timeout 1 tedge mqtt sub --no-topic '${topic}'
    ...    ignore_exit_code=${True}
    ...    strip=${True}
    Should Be Equal    ${messages[0]}    ${retained_message}    msg=MQTT message should be unchanged

    IF    "${c8y_fragment}"
        # There should not be any c8y related operation transition messages sent: https://cumulocity.com/guides/reference/smartrest-two/#updating-operations
        Should Have MQTT Messages
        ...    c8y/s/ds
        ...    message_pattern=^(501|502|503),${c8y_fragment}.*
        ...    minimum=0
        ...    maximum=0
    END
    [Teardown]    Execute Command    tedge mqtt pub --retain '${topic}' ''

Disable config update capability of tedge-agent
    [Arguments]    ${device_sn}=${PARENT_SN}
    Execute Command    tedge config set agent.enable.config_update false
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Service Should Be Running    tedge-agent

Enable config update capability of tedge-agent
    [Arguments]    ${device_sn}=${PARENT_SN}
    Execute Command    tedge config set agent.enable.config_update true
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Service Should Be Running    tedge-agent

Disable config snapshot capability of tedge-agent
    [Arguments]    ${device_sn}=${PARENT_SN}
    Execute Command    tedge config set agent.enable.config_snapshot false
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Service Should Be Running    tedge-agent

Enable config snapshot capability of tedge-agent
    [Arguments]    ${device_sn}=${PARENT_SN}
    Execute Command    tedge config set agent.enable.config_snapshot true
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Service Should Be Running    tedge-agent
