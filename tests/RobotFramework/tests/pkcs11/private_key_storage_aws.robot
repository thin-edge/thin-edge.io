*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 To do this, we install SoftHSM2 which allows us to create software-backed PKCS#11 (cryptoki)
...                 cryptographic tokens that will be read by thin-edge. In real production environments a dedicated
...                 hardware device would be used.

Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             AWS

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           adapter:docker    theme:cryptoki    theme:aws    test:on_demand


*** Test Cases ***
Connect to AWS Using PKCS11 Private Key
    Execute Command    sudo tedge reconnect aws    retries=0
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws
    ThinEdgeIO.Bridge Should Be Up    aws

Connect to AWS and Cumulocity using different keys
    [Documentation]    Verify that we can have two different connections open at the same time to different clouds using
    ...    different tokens/private keys because it's not very secure to use the same keys for different clouds.

    Execute Command    tedge disconnect aws
    Execute Command    tedge disconnect c8y

    # setup: we have two key/cert pairs, each for one cloud
    # backup token & cert we currently use for aws
    Execute Command    mkdir /etc/tedge/device-certs/aws
    Execute Command    mv /etc/tedge/device-certs/tedge-certificate.pem /etc/tedge/device-certs/aws

    Execute Command    tedge config set aws.device.cert_path /etc/tedge/device-certs/aws/tedge-certificate.pem

    # token created in setup by HSM init
    Execute Command    cmd=tedge config set aws.device.key_uri pkcs11:token=tedge

    # c8y setup: create a new token and cert/key and upload it
    Execute Command    tedge cert create --device-id ${DEVICE_SN}
    Set Cumulocity URLs
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y
    ...    log_output=${False}

    Execute Command    softhsm2-util --init-token --free --label c8y-token --pin "123456" --so-pin "123456"
    Execute Command
    ...    bash -c 'softhsm2-util --import <(cat "$(tedge config get device.key_path)" && cat "$(tedge config get device.cert_path)") --token c8y-token --label c8y-key --id 01 --pin 123456'

    Execute Command    mkdir /etc/tedge/device-certs/c8y
    Execute Command    mv /etc/tedge/device-certs/tedge-certificate.pem /etc/tedge/device-certs/c8y
    Execute Command    rm /etc/tedge/device-certs/tedge-private-key.pem

    Execute Command    tedge config set c8y.device.cert_path /etc/tedge/device-certs/c8y/tedge-certificate.pem
    Execute Command    cmd=tedge config set c8y.device.key_uri pkcs11:token=c8y-token

    Restart Service    tedge-p11-server

    # aws bridge takes 30+ seconds to reconnect by default (https://github.com/thin-edge/thin-edge.io/pull/3577#issuecomment-2830774042)
    Execute Command    cmd=tedge config set mqtt.bridge.reconnect_policy.initial_interval 5s

    # act-assert
    Execute Command    tedge connect aws    retries=0
    Execute Command    tedge connect c8y    retries=0

    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

    ThinEdgeIO.Bridge Should Be Up    aws
    ThinEdgeIO.Bridge Should Be Up    c8y


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    $DEVICE_SN

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
