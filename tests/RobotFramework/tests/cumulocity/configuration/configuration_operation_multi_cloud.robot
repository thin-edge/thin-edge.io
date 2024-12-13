*** Settings ***
Resource            ../../../resources/common.resource
Library             OperatingSystem
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Suite Setup
Suite Teardown      Get Logs    name=${DEVICE_SN}

Test Tags           theme:configuration


*** Variables ***
${DEVICE_SN}            ${EMPTY}
${SECOND_DEVICE_SN}     ${EMPTY}


*** Test Cases ***
Get Configuration from Main Device
    [Template]    Get Configuration from Device
    Text file    topic_prefix=c8y    external_id=${DEVICE_SN}    config_type=CONFIG1    device_file=/etc/config1.json
    Binary file    topic_prefix=c8y    external_id=${DEVICE_SN}    config_type=CONFIG1_BINARY    device_file=/etc/binary-config1.tar.gz

Get Configuration from Second Device
    [Template]    Get Configuration from Device
    Text file    topic_prefix=c8y-second    external_id=${SECOND_DEVICE_SN}    config_type=CONFIG1    device_file=/etc/config1.json
    Binary file    topic_prefix=c8y-second    external_id=${SECOND_DEVICE_SN}    config_type=CONFIG1_BINARY    device_file=/etc/binary-config1.tar.gz

Mapper Services Are Restarted After Updates
    Cumulocity.Set Device    ${DEVICE_SN}
    ${pid_before}=    Execute Command    sudo systemctl show --property MainPID tedge-mapper-c8y@second
    Execute Command    dpkg -i packages/tedge-mapper*.deb
    ${pid_after}=    Execute Command    sudo systemctl show --property MainPID tedge-mapper-c8y@second
    Should Not Be Equal    ${pid_before}    ${pid_after}


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
    # Original device
    ${device_sn}=    Setup    skip_bootstrap=${False}
    Set Suite Variable    $DEVICE_SN    ${device_sn}

    Copy Configuration Files    ${DEVICE_SN}
    # "Profiled" device
    Setup Second Device

Setup Second Device
    ${second_device_sn}=    Catenate    SEPARATOR=_    ${device_sn}    second
    Set Suite Variable    $SECOND_DEVICE_SN    ${second_device_sn}

    Execute Command
    ...    tedge config set c8y.device.cert_path --profile second /etc/tedge/device-certs/tedge@second-certificate.pem
    Execute Command
    ...    tedge config set c8y.device.key_path --profile second /etc/tedge/device-certs/tedge@second-key.pem
    Execute Command    tedge config set c8y.proxy.bind.port --profile second 8002
    Execute Command    tedge config set c8y.bridge.topic_prefix --profile second c8y-second
    Execute Command    tedge config set c8y.url --profile second "$(tedge config get c8y.url)"

    Execute Command    tedge cert create --device-id ${second_device_sn} c8y --profile second
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y --profile second

    Execute Command    tedge connect c8y --profile second
    # Verify the mapper has actually started successfully
    Execute Command    systemctl is-active tedge-mapper-c8y@second

    RETURN    ${second_device_sn}

Copy Configuration Files
    [Arguments]    ${device}
    ThinEdgeIO.Set Device Context    ${device}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-configuration-plugin.toml    /etc/tedge/plugins/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config1.json    /etc/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config2.json    /etc/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/binary-config1.tar.gz    /etc/

    # make sure initial files have the same permissions on systems with different umasks
    Execute Command    chmod 664 /etc/config1.json /etc/config2.json /etc/binary-config1.tar.gz
