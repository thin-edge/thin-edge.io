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
