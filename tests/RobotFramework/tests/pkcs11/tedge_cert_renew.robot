*** Settings ***
Documentation       This test suite runs the tests with tedge-p11-server pinned to a fixed version to ensure that new
...                 versions of thin-edge remain backwards compatible with tedge-p11-server's binary communication protocol. The
...                 scope of this test is limited to tedge-p11-server's initial feature set and will generally not be expanded.

Resource            pkcs11_common.resource

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki    compatibility


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     1.6.1


*** Test Cases ***
Use Private Key in SoftHSM2 using tedge-p11-server
    Tedge Reconnect Should Succeed

Renew certificate
    [Template]    Renew certificate using tedge-p11-server version
    ${TEDGE_P11_SERVER_VERSION}    PKCS #11 service failed: Failed to find a signing key
    ${EMPTY}    PKCS #11 service failed: Failed to find a key


*** Keywords ***
Renew certificate using tedge-p11-server version
    [Arguments]    ${version}    ${error}
    Install tedge-p11-server    ${version}
    Execute Command    tedge cert renew c8y
    Tedge Reconnect Should Succeed

    Execute Command    systemctl stop tedge-p11-server tedge-p11-server.socket
    Command Should Fail With
    ...    tedge cert renew c8y
    ...    error=Failed to connect to tedge-p11-server UNIX socket at '/run/tedge-p11-server/tedge-p11-server.sock'

    Execute Command    systemctl start tedge-p11-server.socket

    Execute Command    cmd=tedge config set c8y.device.key_uri pkcs11:object=nonexistent_key
    Command Should Fail With    tedge cert renew c8y    ${error}
    Execute Command    cmd=tedge config unset c8y.device.key_uri

Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    # Allow the tedge user to access softhsm
    Execute Command    sudo usermod -a -G softhsm tedge
    Transfer To Device    ${CURDIR}/data/init_softhsm.sh    /usr/bin/

    # initialize the soft hsm and create a certificate signing request
    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    Execute Command    sudo -u tedge /usr/bin/init_softhsm.sh --device-id "${DEVICE_SN}" --pin 123456

    # configure tedge
    Set Cumulocity URLs
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    ${csr_path}=    Execute Command    cmd=tedge config get device.csr_path    strip=${True}
    Register Device With Cumulocity CA    ${DEVICE_SN}    csr_path=${csr_path}
