*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 To do this, we install SoftHSM2 which allows us to create software-backed PKCS#11 (cryptoki)
...                 cryptographic tokens that will be read by thin-edge. In real production environments a dedicated
...                 hardware device would be used.

Resource            ../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Test Cases ***
Use Private Key in SoftHSM2 using tedge-p11-server
    # initialize the soft hsm and create a self-signed certificate
    Configure tedge-p11-server    module_path=/usr/lib/softhsm/libsofthsm2.so    pin=123456

    # configure tedge
    Execute Command    tedge config set c8y.url "$(echo ${C8Y_CONFIG.host} | sed 's|https?://||g')"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    # Upload the self-signed certificate
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y

    Execute Command    tedge reconnect c8y

Fail if there's no token with given serial available
    # initialize the soft hsm and create a self-signed certificate
    # Configure tedge-p11-server    module_path=/usr/lib/softhsm/libsofthsm2.so    pin=123456    serial=000000000000

    Execute Command    cmd=printf 'TEDGE_DEVICE_CRYPTOKI_MODULE_PATH=%s\nTEDGE_DEVICE_CRYPTOKI_PIN=%s\nTEDGE_DEVICE_CRYPTOKI_SERIAL=%s\n' "/usr/lib/softhsm/libsofthsm2.so" "123456" "000000000000" | sudo tee /etc/tedge/plugins/tedge-p11-server.conf
    Restart Service    tedge-p11-server

    # configure tedge
    Execute Command    tedge config set c8y.url "$(echo ${C8Y_CONFIG.host} | sed 's|https?://||g')"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    # Upload the self-signed certificate
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y

    Run Keyword And Expect Error    *    Execute Command    tedge reconnect c8y

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    # Allow the tedge user to access softhsm
    Execute Command    sudo usermod -a -G softhsm tedge
    Transfer To Device    ${CURDIR}/data/init_softhsm.sh    /usr/bin/
    Remove Existing Certificates
    Execute Command    sudo -u tedge /usr/bin/init_softhsm.sh --self-signed --device-id "${DEVICE_SN}" --pin 123456

Remove Existing Certificates
    Execute Command    cmd=rm -f "$(tedge config get device.key_path)" "$(tedge config get device.cert_path)"

Configure tedge-p11-server
    [Arguments]    ${module_path}    ${pin}    ${serial}=
    Execute Command
    ...    cmd=printf 'TEDGE_DEVICE_CRYPTOKI_MODULE_PATH=%s\nTEDGE_DEVICE_CRYPTOKI_PIN=%s\n' "${module_path}" "${pin}" | sudo tee /etc/tedge/plugins/tedge-p11-server.conf
