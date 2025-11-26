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
    Execute Command    cmd=/usr/bin/tedge-init-hsm.sh --type softhsm2 --label key1
    ThinEdgeIO.Register Device With Cumulocity CA    ${DEVICE_SN}

    ThinEdgeIo.Connect Mapper    c8y


*** Keywords ***
Setup Unregistered Device
    ${DEVICE_SN}=    Setup    register=${False}
    Set Test Variable    ${DEVICE_SN}

    # configure tedge
    Set Cumulocity URLs
