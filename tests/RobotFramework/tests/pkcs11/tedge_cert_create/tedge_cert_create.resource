*** Settings ***
Resource    ../pkcs11_common.resource


*** Keywords ***
Test tedge cert create uses HSM configuration
    # here assumed that we're configured to use PKCS11 private key
    Execute Command    tedge cert remove
    Execute Command    tedge cert create --device-id ${DEVICE_SN}

    Execute Command
    ...    cmd=C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y
    Tedge Reconnect Should Succeed

Tedge cert create without HSM key
    Execute Command    tedge cert remove

    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=nonexistent"
    ${stderr}=    Execute Command
    ...    tedge cert create --device-id ${DEVICE_SN}
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=1
    Should Contain
    ...    ${stderr}
    ...    When using HSM, `tedge cert create` can't create the keypair automatically. Use `tedge cert create-key-hsm` to create the key first.

    Execute Command    cmd=tedge config set device.key_uri "pkcs11:object=nonexistent"
    ${stderr}=    Execute Command
    ...    tedge cert create --device-id ${DEVICE_SN}
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=1
    Should Contain
    ...    ${stderr}
    ...    When using HSM, `tedge cert create` can't create the keypair automatically. Use `tedge cert create-key-hsm` to create the key first.
