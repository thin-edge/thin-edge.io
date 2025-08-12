*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 To do this, we install SoftHSM2 which allows us to create software-backed PKCS#11 (cryptoki)
...                 cryptographic tokens that will be read by thin-edge. In real production environments a dedicated
...                 hardware device would be used.

# it would be good to explain here why we use the tedge-p11-server exclusively and not the module mode
Resource            pkcs11_common.resource

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Test Cases ***
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

    Set tedge-p11-server Uri    value=
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

    Set tedge-p11-server Uri    value=

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
    [Setup]    Set tedge-p11-server Uri    value=${EMPTY}
    [Template]    Connect to C8y using new keypair
    type=rsa    bits=4096
    type=rsa    bits=3072
    type=rsa    bits=2048
    # type=rsa    bits=1024    # RSA 1024 is considered to be insecure is not supported when using the Cumulocity Certificate Authority feature

Connects to C8y supporting all TLS13 ECDSA signature algorithms
    [Documentation]    Check that we support all ECDSA sigschemes used in TLS1.3, i.e: ecdsa_secp256r1_sha256,
    ...    ecdsa_secp384r1_sha384, ecdsa_secp521r1_sha512.
    [Setup]    Set tedge-p11-server Uri    value=${EMPTY}
    [Template]    Connect to C8y using new keypair
    type=ecdsa    curve=secp256r1
    type=ecdsa    curve=secp384r1
    type=ecdsa    curve=secp521r1

Can use PKCS11 key to renew the public certificate
    [Documentation]    Test that `tedge cert renew c8y` works with all supported keys. We do renew 2 times to see if we
    ...    can renew both a self-signed certificate and a certificate signed by C8y CA.
    [Setup]    Set tedge-p11-server Uri    value=${EMPTY}

    Test tedge cert renew    type=ecdsa    curve=secp256r1
    Test tedge cert renew    type=ecdsa    curve=secp384r1

    # renewal isn't supported for secp521r1 because rcgen doesn't support it
    # https://github.com/rustls/rcgen/issues/60

    Test tedge cert renew    type=rsa    bits=2048
    Test tedge cert renew    type=rsa    bits=3072
    Test tedge cert renew    type=rsa    bits=4096

    Execute Command    systemctl stop tedge-p11-server tedge-p11-server.socket
    Command Should Fail With
    ...    tedge cert renew c8y
    ...    error=PEM error: Failed to connect to tedge-p11-server UNIX socket at '/run/tedge-p11-server/tedge-p11-server.sock'

    Execute Command    systemctl start tedge-p11-server.socket
    Execute Command    cmd=tedge config set c8y.device.key_uri pkcs11:object=nonexistent_key
    Command Should Fail With
    ...    tedge cert renew c8y
    ...    error=PEM error: protocol error: bad response, expected sign, received: Error(ProtocolError("PKCS #11 service failed: Failed to find a signing key: Failed to find a private key"))

Ignore tedge.toml if missing
    Execute Command    rm -f ./tedge.toml
    ${stderr}=    Execute Command    tedge-p11-server --config-dir . --module-path xx.so    exp_exit_code=!0
    # Don't log anything (this is normal behaviour as the user does not have to create a tedge.toml file)
    Should Not Contain    ${stderr}    Failed to read ./tedge.toml: No such file
    # And proceed
    Should Contain    ${stderr}    Using cryptoki configuration
    # Using default values
    Should Contain    ${stderr}    tedge-p11-server.sock

Ignore tedge.toml if empty
    Execute Command    touch ./tedge.toml
    ${stderr}=    Execute Command    tedge-p11-server --config-dir . --module-path xx.so    exp_exit_code=!0
    # Don't log anything (this is normal behaviour, where the file is used for tedge and not tedge-p11-server)
    Should Not Contain    ${stderr}    Failed to parse ./tedge.toml: invalid TOML
    # And proceed
    Should Contain    ${stderr}    Using cryptoki configuration
    # Using default values
    Should Contain    ${stderr}    tedge-p11-server.sock

Ignore tedge.toml if incomplete
    Execute Command    echo '[device]' >./tedge.toml
    ${stderr}=    Execute Command    tedge-p11-server --config-dir . --module-path xx.so    exp_exit_code=!0
    # Don't log anything (this is normal behaviour, where the file is used for tedge and not tedge-p11-server)
    Should Not Contain    ${stderr}    Failed to parse ./tedge.toml: invalid TOML
    Should Not Contain    ${stderr}    missing field `cryptoki`
    # And proceed
    Should Contain    ${stderr}    Using cryptoki configuration
    # Using default values
    Should Contain    ${stderr}    tedge-p11-server.sock

Do not warn the user if tedge.toml is incomplete but not used
    Execute Command    rm -f ./tedge.toml
    ${stderr}=    Execute Command
    ...    tedge-p11-server --config-dir . --module-path xx.so --pin 11.pin --socket-path yy.sock --uri zz.uri
    ...    exp_exit_code=!0
    # Don't warn as all values are provided on the command line
    Should Not Contain    ${stderr}    Failed to read ./tedge.toml: No such file
    # And proceed
    Should Contain    ${stderr}    Using cryptoki configuration
    # Using the values provided on the command lin
    Should Contain    ${stderr}    xx.so
    Should Contain    ${stderr}    yy.sock
    Should Contain    ${stderr}    zz.uri

Warn the user if tedge.toml exists but cannot be read
    Execute Command    echo '[device.cryptoki]' >./tedge.toml
    Execute Command    chmod a-rw ./tedge.toml
    ${stderr}=    Execute Command
    ...    sudo -u tedge tedge-p11-server --config-dir . --module-path xx.so
    ...    exp_exit_code=!0
    # Warn the user
    Should Contain    ${stderr}    Failed to read ./tedge.toml: Permission denied
    # But proceed
    Should Contain    ${stderr}    Using cryptoki configuration

Warn the user if tedge.toml cannot be parsed
    Execute Command    rm -f ./tedge.toml
    Execute Command    echo '[corrupted toml ...' >./tedge.toml
    ${stderr}=    Execute Command    tedge-p11-server --config-dir . --module-path xx.so    exp_exit_code=!0
    # Warn the user
    Should Contain    ${stderr}    Failed to parse ./tedge.toml: invalid TOML
    # But proceed
    Should Contain    ${stderr}    Using cryptoki configuration


*** Keywords ***
Test tedge cert renew
    [Arguments]    ${type}    ${bits}=${EMPTY}    ${curve}=${EMPTY}

    Connect to C8y using new keypair    type=${type}    curve=${curve}    bits=${bits}
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
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    # Allow the tedge user to access softhsm
    Execute Command    sudo usermod -a -G softhsm tedge
    Transfer To Device    ${CURDIR}/data/init_softhsm.sh    /usr/bin/

    # initialize the soft hsm and create a certificate signing request
    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    Execute Command    sudo -u tedge /usr/bin/init_softhsm.sh --device-id "${DEVICE_SN}" --pin 123456

    # configure tedge
    ${domain}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${domain}"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    ${csr_path}=    Execute Command    cmd=tedge config get device.csr_path    strip=${True}
    Register Device With Cumulocity CA    ${csr_path}

    Set tedge-p11-server Uri    value=
