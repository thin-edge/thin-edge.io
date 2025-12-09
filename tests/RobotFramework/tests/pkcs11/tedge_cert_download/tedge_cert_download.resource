*** Settings ***
Resource    ../pkcs11_common.resource


*** Keywords ***
Use tedge cert download c8y to download a certificate
    [Documentation]    Download a certificate using CSR generated with PKCS11 without a prior certificate.
    # this new keypair doesn't have an associated certificate
    Set up new PKCS11 ECDSA keypair

    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}
    Execute Command
    ...    cmd=tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 60s

    Tedge Reconnect Should Succeed
