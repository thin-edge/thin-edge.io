*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 To do this, we install SoftHSM2 which allows us to create software-backed PKCS#11 (cryptoki)
...                 cryptographic tokens that will be read by thin-edge. In real production environments a dedicated
...                 hardware device would be used.

Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             AWS

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           adapter:docker    theme:cryptoki    theme:aws    test:on_demand


*** Test Cases ***
Connect to AWS Using PKCS11 Private Key
    Execute Command    sudo tedge reconnect aws    retries=0
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws
    ThinEdgeIO.Bridge Should Be Up    aws


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
    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    Execute Command    sudo -u tedge /usr/bin/init_softhsm.sh --self-signed --device-id "${DEVICE_SN}" --pin 123456

    # configure tedge
    ${aws_url}=    AWS.Get IoT URL
    Execute Command    sudo tedge config set aws.url ${aws_url}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    # Upload the self-signed certificate
    ${cert_contents}=    Execute Command    cat $(tedge config get device.cert_path)
    ${aws}=    AWS.Create Thing With Self-Signed Certificate    name=${DEVICE_SN}    certificate_pem=${cert_contents}

Remove Existing Certificates
    Execute Command    cmd=rm -f "$(tedge config get device.key_path)" "$(tedge config get device.cert_path)"
