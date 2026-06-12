*** Settings ***
Documentation       Custom operations receive the mapper's cloud profile via the
...                 TEDGE_CLOUD_PROFILE environment variable. A mapper running for a
...                 named profile must expose that profile to the operation script,
...                 while the default (unnamed) profile must leave it unset.

Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs    name=${DEVICE_SN}

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Variables ***
${DEVICE_SN}            ${EMPTY}
${SECOND_DEVICE_SN}     ${EMPTY}


*** Test Cases ***
Custom operation on the default profile has no cloud profile
    Cumulocity.Set Device    ${DEVICE_SN}
    ${operation}=    Cumulocity.Create Operation
    ...    description=print cloud profile
    ...    fragments={"c8y_Command":{"text":"print profile"}}
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    profile=

Custom operation on a named profile receives the cloud profile
    Cumulocity.Set Device    ${SECOND_DEVICE_SN}
    ${operation}=    Cumulocity.Create Operation
    ...    description=print cloud profile
    ...    fragments={"c8y_Command":{"text":"print profile"}}
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    profile=second


*** Keywords ***
Custom Setup
    # Original device (default profile)
    ${device_sn}=    Setup
    Set Suite Variable    $DEVICE_SN    ${device_sn}
    Device Should Exist    ${DEVICE_SN}
    Install Custom Operation

    # "Profiled" device
    Setup Second Device

Install Custom Operation
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command    /etc/tedge/operations/c8y/c8y_Command
    ThinEdgeIO.Transfer To Device    ${CURDIR}/print_profile.sh    /etc/tedge/operations/print_profile
    Execute Command    chmod a+x /etc/tedge/operations/print_profile

Setup Second Device
    ${second_device_sn}=    Catenate    SEPARATOR=_    ${DEVICE_SN}    second
    Set Suite Variable    $SECOND_DEVICE_SN    ${second_device_sn}

    Execute Command
    ...    tedge config set c8y.device.cert_path --profile second /etc/tedge/device-certs/tedge@second-certificate.pem
    Execute Command
    ...    tedge config set c8y.device.key_path --profile second /etc/tedge/device-certs/tedge@second-key.pem
    Execute Command    tedge config set c8y.proxy.bind.port --profile second 8002
    Execute Command    tedge config set c8y.bridge.topic_prefix --profile second c8y-second

    Set Cumulocity URLs    profile=second

    Execute Command    tedge cert create --device-id ${second_device_sn} c8y --profile second
    Register Certificate For Cleanup    cloud_profile=second    common_name=${second_device_sn}
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y --profile second

    Execute Command    tedge connect c8y --profile second
    # Verify the mapper has actually started successfully
    Execute Command    systemctl is-active tedge-mapper-c8y@second

    Device Should Exist    ${second_device_sn}
    RETURN    ${second_device_sn}
