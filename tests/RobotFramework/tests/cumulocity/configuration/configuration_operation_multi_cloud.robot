*** Settings ***
Resource            ../../../resources/common.resource
Library             OperatingSystem
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Suite Setup
Suite Teardown      Get Logs    name=${PARENT_SN}
Test Setup          Test Setup

Test Tags           theme:configuration    theme:childdevices


*** Variables ***
${PARENT_SN}            ${EMPTY}
${SECOND_DEVICE_SN}     ${EMPTY}


*** Test Cases ***    DEVICE    EXTERNALID    CONFIG_TYPE    DEVICE_FILE    FILE    PERMISSION    OWNERSHIP
#
# Get configuration
#

Get Configuration from Main Device
    [Template]    Get Configuration from Device
    Text file    topic_prefix=c8y    external_id=${PARENT_SN}    config_type=CONFIG1    device_file=/etc/config1.json
    Binary file    topic_prefix=c8y    external_id=${PARENT_SN}    config_type=CONFIG1_BINARY    device_file=/etc/binary-config1.tar.gz

Get Configuration from Second Device
    [Template]    Get Configuration from Device
    Text file    topic_prefix=c8y-second    external_id=${SECOND_DEVICE_SN}    config_type=CONFIG1    device_file=/etc/config1.json
    Binary file    topic_prefix=c8y-second    external_id=${SECOND_DEVICE_SN}    config_type=CONFIG1_BINARY    device_file=/etc/binary-config1.tar.gz


*** Keywords ***
Get Configuration from Device
    [Arguments]    ${description}    ${topic_prefix}    ${external_id}    ${config_type}    ${device_file}
    Log    Test Description: ${description}
    Cumulocity.Set Device    ${external_id}
    ${operation}=    Cumulocity.Get Configuration    ${config_type}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=20

    ${expected_checksum}=    Execute Command    md5sum '${device_file}' | cut -d' ' -f1    strip=${True}
    ${events}=    Cumulocity.Device Should Have Event/s
    ...    minimum=1
    ...    maximum=1
    ...    type=${config_type}
    ...    with_attachment=${True}
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${events[0]["id"]}
    ...    expected_md5=${expected_checksum}

    ${event}=    Cumulocity.Event Attachment Should Have File Info
    ...    ${events[0]["id"]}
    ...    name=^${external_id}_[\\w\\W]+${topic_prefix}-mapper-\\d+$

    RETURN    ${contents}

#
# Setup
#

Suite Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=${False}
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    # Child
    Setup Second Device

Setup Second Device
    ${child_sn}=    Catenate    SEPARATOR=_    ${parent_sn}    second
    Set Suite Variable    $SECOND_DEVICE_SN    ${child_sn}

    Execute Command
    ...    tedge config set c8y@second.device.cert_path /etc/tedge/device-certs/tedge@second-certificate.pem
    Execute Command    tedge config set c8y@second.device.key_path /etc/tedge/device-certs/tedge@second-key.pem
    Execute Command    tedge config set c8y@second.proxy.bind.port 8002
    Execute Command    tedge config set c8y@second.bridge.topic_prefix c8y-second
    Execute Command    tedge config set c8y@second.url "$(tedge config get c8y.url)"

    Execute Command    tedge cert create c8y@second --device-id ${child_sn}
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y@second

    Execute Command    tedge connect c8y@second

    RETURN    ${child_sn}

Test Setup
    Customize Operation Workflows    ${PARENT_SN}
    Copy Configuration Files    ${PARENT_SN}

Copy Configuration Files
    [Arguments]    ${device}
    ThinEdgeIO.Set Device Context    ${device}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-configuration-plugin.toml    /etc/tedge/plugins/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config1.json    /etc/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config2.json    /etc/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/binary-config1.tar.gz    /etc/

    # make sure initial files have the same permissions on systems with different umasks
    Execute Command    chmod 664 /etc/config1.json /etc/config2.json /etc/binary-config1.tar.gz

    # on a child device, user with uid 1000 doesn't exist, so make sure files we're testing on have a well defined user
    Execute Command
    ...    chown root:root /etc/tedge/plugins/tedge-configuration-plugin.toml /etc/config1.json /etc/binary-config1.tar.gz

Customize Operation Workflows
    [Arguments]    ${device}
    ThinEdgeIO.Set Device Context    ${device}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/sub_config_snapshot.toml    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/sub_config_update.toml    /etc/tedge/operations/
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
