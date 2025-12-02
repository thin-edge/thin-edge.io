*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 This suite focuses on testing selection and connecting to the cloud using different types of private
...                 keys stored in PKCS#11 tokens.
...
...                 Uses SoftHSM2 to simulate a hardware security module for testing purposes. In real production
...                 environments, a dedicated hardware device would be used.

# it would be good to explain here why we use the tedge-p11-server exclusively and not the module mode
Resource            ../pkcs11_common.resource


*** Variables ***
${KEY_URI}      ${EMPTY}


*** Keywords ***
Use Private Key in SoftHSM2 using tedge-p11-server
    Tedge Reconnect Should Succeed

Select Private key using tedge-p11-server URI
    [Documentation]    Make sure that we can select different keys and tokens using a PKCS#11 URI.
    ...    The URI can either point to a specific key, or to a specific token where we will attempt to find a key.
    ...
    ...    To ensure that correct key is selected and reduce the need to generate and upload different keys and
    ...    certificates, we'll only be using one key and we'll only import it to the chosen token, keeping other tokens
    ...    empty.
    ...
    ...    We set the URI on tedge-p11-server, which means that all connecting clients will use the selected key until
    ...    tedge-p11-server is restarted with a different URI.

    Unset tedge-p11-server Uri
    Tedge Reconnect Should Succeed

    # expect failure if we try to use a token that doesn't exist
    Set tedge-p11-server Uri    value=pkcs11:token=asdf
    Tedge Reconnect Should Fail With    Failed to find a signing key: Didn't find a slot to use

    # create tokens with no keys on them, so key selection fails if wrong token is selected
    Execute Command    softhsm2-util --init-token --free --label token1 --pin "123456" --so-pin "123456"

    Set tedge-p11-server Uri    value=pkcs11:token=token1
    Tedge Reconnect Should Fail With    Failed to find a signing key

    Set tedge-p11-server Uri    value=pkcs11:token=tedge
    Tedge Reconnect Should Succeed

    # import another private key to the primary token (one that has valid tedge key) so we can select a key
    Execute Command
    ...    cmd=p11tool --set-pin=123456 --login --generate-privkey ECDSA --curve=secp256r1 --label "key2" "pkcs11:token=tedge"

    Set tedge-p11-server Uri    value=pkcs11:token=tedge;object=key2
    Tedge Reconnect Should Fail With    HandshakeFailure

    # but when URI has correct label, we expect valid key to be used again
    Set tedge-p11-server Uri    value=pkcs11:token=tedge;object=tedge
    Tedge Reconnect Should Succeed
    [Teardown]    Unset tedge-p11-server Uri

Select Private key using a request URI
    [Documentation]    Like above, we select the key using a URI, but this time we include it in a request, which means
    ...    we can select different keys without restarting tedge-p11-server.

    Execute Command    cmd=tedge config set device.key_uri pkcs11:token=token123
    ${stderr}=    Tedge Reconnect Should Fail With    Failed to find a signing key
    Should Contain    ${stderr}    item=cryptoki: socket (key: pkcs11:token=token123)

    Execute Command    cmd=tedge config unset device.key_uri
    Execute Command    cmd=tedge config set device.key_uri pkcs11:token=token123
    ${stderr}=    Tedge Reconnect Should Fail With    Failed to find a signing key
    Should Contain    ${stderr}    item=cryptoki: socket (key: pkcs11:token=token123)

    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=tedge;object=tedge"
    ${stderr}=    Tedge Reconnect Should Succeed
    Should Contain    ${stderr}    item=cryptoki: socket (key: pkcs11:token=tedge;object=tedge)

Connects to C8y using an RSA key
    [Documentation]    Test that we can connect to C8y using an RSA private keys of all sizes.
    [Setup]    Unset tedge-p11-server Uri
    Connect to C8y using new RSA keypair    bits=4096
    Connect to C8y using new RSA keypair    bits=3072
    Connect to C8y using new RSA keypair    bits=2048

Connects to C8y supporting all TLS13 ECDSA signature algorithms
    [Documentation]    Check that we support all ECDSA sigschemes used in TLS1.3, i.e: ecdsa_secp256r1_sha256,
    ...    ecdsa_secp384r1_sha384, ecdsa_secp521r1_sha512.
    [Setup]    Unset tedge-p11-server Uri

    Connect to C8y using new ECDSA keypair    curve=secp256r1
    Connect to C8y using new ECDSA keypair    curve=secp384r1
    # rcgen doesn't support p521
    # https://github.com/rustls/rcgen/issues/60
    # Connect to C8y using new ECDSA keypair    curve=p521

Use PKCS11 key to renew the public certificate
    [Documentation]    Test that `tedge cert renew c8y` works with all supported keys. We do renew 2 times to see if we
    ...    can renew both a self-signed certificate and a certificate signed by C8y CA.
    [Setup]    Unset tedge-p11-server Uri

    Test tedge cert renew    type=ecdsa    curve=secp256r1
    Test tedge cert renew    type=ecdsa    curve=secp384r1

    # renewal isn't supported for secp521r1 because rcgen doesn't support it
    # https://github.com/rustls/rcgen/issues/60

    Test tedge cert renew    type=rsa    bits=2048
    Test tedge cert renew    type=rsa    bits=3072
    Test tedge cert renew    type=rsa    bits=4096

Use tedge cert download c8y to download a certificate
    [Documentation]    Download a certificate using CSR generated with PKCS11 without a prior certificate.
    # this new keypair doesn't have an associated certificate
    Set up new PKCS11 ECDSA keypair

    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}
    Execute Command
    ...    cmd=tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 60s

    Tedge Reconnect Should Succeed

Renew the certificate using different keypair
    [Documentation]    Starting with an initial trusted certificate, replace the keypair and renew the certificate.
    Connect to C8y using new ECDSA keypair
    Set up new PKCS11 ECDSA keypair
    Execute Command    tedge cert renew c8y
    ${stdout}=    Tedge Reconnect Should Succeed
    Should Contain    ${stdout}    The new certificate is now the active certificate

Pass PIN in the request using pin-value
    [Documentation]    Tests if the PIN can be changed for the request by assuming current one is correct and setting a
    ...    different one to see if we get an error about pin being incorrect.

    ${key_uri}=    Execute Command    tedge config get device.key_uri    strip=True    ignore_exit_code=True
    # FIXME: this breaks if currently set URI already has query attributes, but currently that's not the case (other tests don't set it)
    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=tedge;object=tedge?pin-value=incorrect"
    Tedge Reconnect Should Fail With    The specified PIN is incorrect

    [Teardown]    Execute Command    tedge config set device.key_uri "${key_uri}"

Pass PIN in the request using device.key_pin
    Execute Command    tedge config set device.key_pin incorrect
    Tedge Reconnect Should Fail With    The specified PIN is incorrect

    [Teardown]    Execute Command    tedge config unset device.key_pin

Test tedge cert renew
    [Arguments]    ${type}    ${bits}=${EMPTY}    ${curve}=${EMPTY}
    IF    $type == "rsa"
        Connect to C8y using new RSA keypair    ${bits}
    ELSE
        Connect to C8y using new ECDSA keypair    ${curve}
    END
    # We could alternatively use Cumulocity CA to start with a signed cert, but for testing certificate renewal, we want
    # to test both renewing a self-signed cert and a cert issued by C8y CA. When we start with self-signed cert, after
    # the first renewal we get a cert signed by CA, so we test all scenarios by just doing renew 2 times.

    Execute Command    tedge cert renew c8y
    ${stderr}=    Execute Command
    ...    openssl req -text -noout -in /etc/tedge/device-certs/tedge.csr -verify
    ...    stdout=False
    ...    stderr=true
    Should Contain    ${stderr}    Certificate request self-signature verify OK

    Tedge Reconnect Should Succeed

    Execute Command    tedge cert renew c8y
    ${stderr}=    Execute Command
    ...    openssl req -text -noout -in /etc/tedge/device-certs/tedge.csr -verify
    ...    stdout=False
    ...    stderr=true
    Should Contain    ${stderr}    Certificate request self-signature verify OK

    Tedge Reconnect Should Succeed

Custom Setup
    [Arguments]    ${tedge_p11_server_version}
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    # initialize the soft hsm
    Execute Command    sudo /usr/bin/tedge-init-hsm.sh --type softhsm2 --pin 123456
    # tests expect that the device.key_uri is initially unset
    Execute Command    cmd=tedge config unset device.key_uri

    # configure tedge
    Set Cumulocity URLs
    ThinEdgeIO.Register Device With Cumulocity CA    ${DEVICE_SN}

    Install tedge-p11-server    ${tedge_p11_server_version}

    Unset tedge-p11-server Uri
