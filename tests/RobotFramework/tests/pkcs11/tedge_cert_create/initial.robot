*** Settings ***
Resource        tedge_cert_create.resource

Suite Setup     tedge-p11-server Setup    ${TEDGE_P11_SERVER_VERSION}


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     1.7.0


*** Test Cases ***
Tedge cert create should use HSM configuration
    Test tedge cert create uses HSM configuration

Tedge cert create should tell user to manually create key if it's not present
    Tedge cert create without HSM key

Tedge cert create should report if tedge-p11-server is too old
    Install tedge-p11-server    1.6.0

    Execute Command    tedge cert remove
    ${stderr}=    Execute Command
    ...    tedge cert create --device-id ${DEVICE_SN}
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=!0

    Should Contain    ${stderr}    0: can't use HSM private key to sign the certificate
    Should Contain    ${stderr}    1: Failed to obtain PEM of PKCS11 public key
    Should Contain
    ...    ${stderr}
    ...    2: tedge-p11-server wasn't able to understand the request, perhaps because its version is too old
