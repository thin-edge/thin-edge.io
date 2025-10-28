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


*** Variables ***
${KEY_URI}      ${EMPTY}


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
    [Template]    Connect to C8y using new keypair
    type=rsa    bits=4096
    type=rsa    bits=3072
    type=rsa    bits=2048
    # type=rsa    bits=1024    # RSA 1024 is considered to be insecure is not supported when using the Cumulocity Certificate Authority feature

Connects to C8y supporting all TLS13 ECDSA signature algorithms
    [Documentation]    Check that we support all ECDSA sigschemes used in TLS1.3, i.e: ecdsa_secp256r1_sha256,
    ...    ecdsa_secp384r1_sha384, ecdsa_secp521r1_sha512.
    [Setup]    Unset tedge-p11-server Uri

    Connect to C8y using new keypair    type=ecdsa    curve=secp256r1
    Connect to C8y using new keypair    type=ecdsa    curve=secp384r1
    Connect to C8y using new keypair    type=ecdsa    curve=secp521r1

    Execute Command    systemctl stop tedge-p11-server tedge-p11-server.socket
    Command Should Fail With
    ...    tedge cert renew c8y
    ...    error=Failed to connect to tedge-p11-server UNIX socket at '/run/tedge-p11-server/tedge-p11-server.sock'

    Execute Command    systemctl start tedge-p11-server.socket

    Execute Command    cmd=tedge config set c8y.device.key_uri pkcs11:object=nonexistent_key
    Command Should Fail With
    ...    tedge cert renew c8y
    ...    error=PKCS #11 service failed: Failed to find a key
    Execute Command    cmd=tedge config unset c8y.device.key_uri

Can use PKCS11 key to renew the public certificate
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

Can use tedge cert download c8y to download a certificate
    [Documentation]    Download a certificate using CSR generated with PKCS11 without a prior certificate.
    # this new keypair doesn't have an associated certificate
    Set up new PKCS11 keypair    type=ecdsa

    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}
    Execute Command
    ...    cmd=tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 60s

    Tedge Reconnect Should Succeed

Can renew the certificate using different keypair
    [Documentation]    Starting with an initial trusted certificate, replace the keypair and renew the certificate.
    Connect to C8y using new keypair    type=ecdsa
    Set up new PKCS11 keypair    type=ecdsa
    Execute Command    tedge cert renew c8y
    ${stdout}=    Tedge Reconnect Should Succeed
    Should Contain    ${stdout}    The new certificate is now the active certificate

Can pass PIN in the request using pin-value
    [Documentation]    Tests if the PIN can be changed for the request by assuming current one is correct and setting a
    ...    different one to see if we get an error about pin being incorrect.

    ${key_uri}=    Execute Command    tedge config get device.key_uri    strip=True    ignore_exit_code=True
    # FIXME: this breaks if currently set URI already has query attributes, but currently that's not the case (other tests don't set it)
    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=tedge;object=tedge?pin-value=incorrect"
    Tedge Reconnect Should Fail With    The specified PIN is incorrect

    [Teardown]    Execute Command    tedge config set device.key_uri "${key_uri}"

Can pass PIN in the request using device.key_pin
    Execute Command    tedge config set device.key_pin incorrect
    Tedge Reconnect Should Fail With    The specified PIN is incorrect

    [Teardown]    Execute Command    tedge config unset device.key_pin

Can create a private key on the PKCS11 token
    Execute Command    cmd=softhsm2-util --init-token --free --label create-key-token --pin=123456 --so-pin=123456

    ${output}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "pkcs11:token=create-key-token"
    ...    exp_exit_code=!0
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Be Equal    ${output}    No matching objects found

    Set tedge-p11-server Uri    value=pkcs11:token=create-key-token

    Create private key    label=rsa-2048    type=rsa    p11tool_keytype=RSA-2048
    Create private key
    ...    label=rsa-3072
    ...    type=rsa
    ...    bits=3072
    ...    p11tool_keytype=RSA-3072
    Create private key
    ...    label=rsa-4096
    ...    type=rsa
    ...    bits=4096
    ...    p11tool_keytype=RSA-4096

    Create private key
    ...    label=ec-256
    ...    type=ecdsa
    ...    curve=p256
    ...    p11tool_keytype=EC/ECDSA-SECP256R1
    Create private key
    ...    label=ec-384
    ...    type=ecdsa
    ...    curve=p384
    ...    p11tool_keytype=EC/ECDSA-SECP384R1
    # ECDSA P521 not supported by rcgen

    [Teardown]    Set tedge-p11-server Uri    value=

tedge cert create-key-pkcs11 should ask where to create keypair if multiple tokens available
    # setup multiple tokens
    Execute Command    cmd=softhsm2-util --init-token --free --label create-key-token1 --pin=123456 --so-pin=123456
    Execute Command    cmd=softhsm2-util --init-token --free --label create-key-token2 --pin=123456 --so-pin=123456

    # unset key_uri so there there's no hint where to generate the keypair
    Execute Command    cmd=tedge config unset device.key_uri
    ${stderr}=    Execute Command    cmd=tedge cert create-key-pkcs11 --type ecdsa --label my-key    strip=True    stdout=False    stderr=True    exp_exit_code=1
    Should Contain    ${stderr}    No token URL was provided for this operation; the available tokens are:
    Should Contain    ${stderr}    token=create-key-token1
    Should Contain    ${stderr}    token=create-key-token2

tedge cert create-key-pkcs11 can set chosen id and returns error if object with this id already exists
    ${output}=    Execute Command    cmd=tedge cert create-key-pkcs11 --type ecdsa --label my-key --id 010203 "pkcs11:token=tedge"    strip=True    stdout=False    stderr=True
    Should Contain    ${output}    id=%01%02%03

    ${output}=    Execute Command    cmd=tedge cert create-key-pkcs11 --type ecdsa --label my-key --id 010203 "pkcs11:token=tedge"    strip=True    stdout=False    stderr=True    exp_exit_code=!0
    Should Contain    ${output}    Object with this id already exists on the token

tedge cert create-key-pkcs11 can set pin per request
    ${output}=    Execute Command    cmd=tedge cert create-key-pkcs11 --label my-key --pin 000000 "pkcs11:token=tedge"    strip=True    stdout=False    stderr=True    exp_exit_code=!0
    Should Contain    ${output}    The specified PIN is incorrect

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
Create private key
    [Arguments]    ${type}    ${label}    ${bits}=${EMPTY}    ${curve}=${EMPTY}    ${p11tool_keytype}=${EMPTY}
    # create the private key on token and write CSR to device.csr_path
    VAR    ${command}=    tedge cert create-key-pkcs11 --label ${label} --type ${type} "pkcs11:token=create-key-token"
    IF    $bits
        VAR    ${command}=    ${command} --bits ${bits}
    END
    IF    $curve
        VAR    ${command}=    ${command} --curve ${curve}
    END
    ${create_key_output}=    Execute Command    ${command}    strip=True    stderr=True    stdout=False

    # check if key is created
    ${output}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "pkcs11:token=create-key-token"
    IF    $p11tool_keytype
        Should Contain    ${output}    Type: Private key (${p11tool_keytype})
    ELSE
        Should Contain    ${output}    Type: Private key
    END
    Should Contain    ${output}    Label: ${label}

    ${key_uri}=    Execute Command    tedge config get device.key_uri    strip=True
    Should Contain    ${create_key_output}    ${key_uri}

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
    ThinEdgeIO.Register Device With Cumulocity CA    ${DEVICE_SN}    csr_path=${csr_path}

    Unset tedge-p11-server Uri
