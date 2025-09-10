*** Settings ***
Resource            ./pkcs11_common.resource

Test Teardown       Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Test Cases ***
Use new pkcs11 key without restarting tedge-p11-server
    [Documentation]    When using client side pkcs11 uri, the tedge-p11-server should not have
    ...    to be restarted in order to be used. This is essential when the tedge-p11-server
    ...    is running on the host, and tedge is running in a container, where tedge has no control
    ...    over the host
    [Setup]    Setup Unregistered Device

    # Nothing should work as no-private key should exist
    pkcs11_common.Tedge Reconnect Should Fail With    Config value device.id, cannot be read

    # create tokens with no keys on them, so key selection fails if wrong token is selected
    Execute Command    init_softhsm.sh --device-id ${DEVICE_SN} --label key1
    ThinEdgeIO.Register Device With Cumulocity CA    ${DEVICE_SN}    csr_path=/etc/tedge/device-certs/tedge.csr

    Execute Command
    ...    cmd=tedge config set device.key_uri "pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;token=key1"

    ThinEdgeIo.Connect Mapper    c8y


*** Keywords ***
Setup Unregistered Device
    ${DEVICE_SN}=    Setup    register=${False}
    Set Test Variable    ${DEVICE_SN}

    # Allow the tedge user to access softhsm
    Execute Command    sudo usermod -a -G softhsm tedge
    Transfer To Device    ${CURDIR}/data/init_softhsm.sh    /usr/bin/

    # initialize the soft hsm and create a certificate signing request
    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    Restart Service    tedge-p11-server

    # configure tedge
    ${domain}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${domain}"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket
