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
    Tedge Reconnect Should Succeed

Select Private key using PKCS#11 URI
    [Documentation]    Make sure that we can select different keys and tokens using a PKCS#11 URI.
    ...    The URI can either point to a specific key, or to a specific token where we will attempt to find a key.
    ...
    ...    To ensure that correct key is selected and reduce the need to generate and upload different keys and
    ...    certificates, we'll only be using one key and we'll only import it to the chosen token, keeping other tokens
    ...    empty.

    Tedge Reconnect Should Succeed

    # expect failure if we try to use a token that doesnt exist
    Set tedge-p11-server Configuration    pkcs11_url=pkcs11:token=asdf
    Tedge Reconnect Should Fail With    Failed to find a signing key: Didn't find a slot to use

    # create tokens with no keys on them, so key selection fails if wrong token is selected
    Execute Command    softhsm2-util --init-token --free --label token1 --pin "123456" --so-pin "123456"

    Set tedge-p11-server Configuration    pkcs11_url=pkcs11:token=token1
    Tedge Reconnect Should Fail With    Failed to find a signing key

    Set tedge-p11-server Configuration    pkcs11_url=pkcs11:token=tedge
    Tedge Reconnect Should Succeed

    # import another private key to the primary token (one that has valid tedge key) so we can select a key
    Execute Command
    ...    cmd=p11tool --set-pin=123456 --login --generate-privkey ECDSA --curve=secp256r1 --label "key2" "pkcs11:token=tedge"

    Set tedge-p11-server Configuration    pkcs11_url=pkcs11:token=tedge;object=key2
    Tedge Reconnect Should Fail With    HandshakeFailure

    # but when URI has correct label, we expect valid key to be used again
    Set tedge-p11-server Configuration    pkcs11_url=pkcs11:token=tedge;object=tedge
    Tedge Reconnect Should Succeed


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    # Allow the tedge user to access softhsm
    Execute Command    sudo usermod -a -G softhsm tedge
    Transfer To Device    ${CURDIR}/data/init_softhsm.sh    /usr/bin/
    Remove Existing Certificates

    # initialize the soft hsm and create a self-signed certificate
    Configure tedge-p11-server    module_path=/usr/lib/softhsm/libsofthsm2.so    pin=123456
    Execute Command    sudo -u tedge /usr/bin/init_softhsm.sh --self-signed --device-id "${DEVICE_SN}" --pin 123456

    # configure tedge
    Execute Command    tedge config set c8y.url "$(echo ${C8Y_CONFIG.host} | sed 's|https?://||g')"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    # Upload the self-signed certificate
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y

Remove Existing Certificates
    Execute Command    cmd=rm -f "$(tedge config get device.key_path)" "$(tedge config get device.cert_path)"

Configure tedge-p11-server
    [Arguments]    ${module_path}    ${pin}
    Execute Command
    ...    cmd=printf 'TEDGE_DEVICE_CRYPTOKI_MODULE_PATH=%s\nTEDGE_DEVICE_CRYPTOKI_PIN=%s\n' "${module_path}" "${pin}" | sudo tee /etc/tedge/plugins/tedge-p11-server.conf

Set tedge-p11-server Configuration
    [Arguments]    ${pkcs11_url}
    Execute Command
    ...    cmd=printf 'TEDGE_DEVICE_CRYPTOKI_MODULE_PATH=%s\nTEDGE_DEVICE_CRYPTOKI_PIN=%s\nTEDGE_DEVICE_CRYPTOKI_URI=%s\n' "/usr/lib/softhsm/libsofthsm2.so" "123456" "${pkcs11_url}" | sudo tee /etc/tedge/plugins/tedge-p11-server.conf
    Restart Service    tedge-p11-server

Tedge Reconnect Should Succeed
    Execute Command    tedge reconnect c8y

Tedge Reconnect Should Fail With
    [Arguments]    ${error}
    ${stderr}=    Execute Command    tedge reconnect c8y    exp_exit_code=!0    stdout=false    stderr=true
    Should Contain    ${stderr}    ${error}
