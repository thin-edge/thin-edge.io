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

Fail cert download when key_uri does not select any of the existing keys
    [Documentation]    When the token holds keys but none matches the configured `key_uri`,
    ...    `tedge cert download c8y` must not create another key automatically; it fails and lists
    ...    the existing keys so one can be selected.
    Execute Command    cmd=tedge config set c8y.device.key_uri "pkcs11:token=tedge;object=nonexistent-key"

    ${stderr}=    Execute Command
    ...    cmd=tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password 'dummy' --retry-every 5s --max-timeout 10s
    ...    exp_exit_code=!0
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    the PKCS #11 token holds other private keys
    Should Contain    ${stderr}    tedge hsm create-key
    [Teardown]    Execute Command    cmd=tedge config unset c8y.device.key_uri

Automatically create a key when downloading a certificate and the token holds none
    [Documentation]    On a fresh HSM without any key (or even an initialized token),
    ...    `tedge cert download c8y` initializes the token and creates a key automatically, so a
    ...    device can be provisioned with a single command.
    # Start from an HSM with no initialized token and no configured key
    Execute Command    cmd=softhsm2-util --delete-token --token tedge
    Execute Command    cmd=tedge config unset device.key_uri
    Execute Command    cmd=tedge config unset c8y.device.key_uri
    Restart Service    tedge-p11-server

    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}
    ${stderr}=    Execute Command
    ...    cmd=tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 60s
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    No private key was found on the PKCS #11 token; creating one automatically.
    Should Contain    ${stderr}    Certificate downloaded successfully

    # the created key was selected for use with c8y
    ${key_uri}=    Execute Command    tedge config get c8y.device.key_uri    strip=True
    Should Start With    ${key_uri}    pkcs11:
    Should Contain    ${key_uri}    object=tedge

    Tedge Reconnect Should Succeed
