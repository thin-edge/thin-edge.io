*** Settings ***
Resource        tedge_cert_download.resource

Suite Setup     tedge-p11-server Setup    ${TEDGE_P11_SERVER_VERSION}


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     1.7.0
${PKCS11_USE_P11TOOL}           ${True}


*** Test Cases ***
Can use tedge cert download c8y to download a certificate
    Use tedge cert download c8y to download a certificate

Tedge cert download error
    Set up new PKCS11 ECDSA keypair
    Install tedge-p11-server    1.6.1
    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}
    ${stderr}=    Execute Command
    ...    cmd=tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 60s
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=!0
    Should Contain    ${stderr}    0: Fail to create the device CSR /etc/tedge/device-certs/tedge.csr
    Should Contain    ${stderr}    1: Failed to obtain PEM of PKCS11 public key
    Should Contain
    ...    ${stderr}
    ...    2: tedge-p11-server wasn't able to understand the request, perhaps because its version is too old
