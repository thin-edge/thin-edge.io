*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 To do this, we install SoftHSM2 which allows us to create software-backed PKCS#11 (cryptoki)
...                 cryptographic tokens that will be read by thin-edge. In real production environments a dedicated
...                 hardware device would be used.

# it would be good to explain here why we use the tedge-p11-server exclusively and not the module mode
Resource            ../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Variables ***
${CERT_TEMPLATE}    /etc/tedge/hsm/cert.template    # created by init_softhsm script


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
    ${stdout}=    Tedge Reconnect Should Fail With    Failed to find a signing key
    Should Contain    ${stdout}    item=cryptoki: socket (key: pkcs11:token=token123)

    Execute Command    cmd=tedge config unset device.key_uri
    Execute Command    cmd=tedge config set c8y.device.key_uri pkcs11:token=token123
    ${stdout}=    Tedge Reconnect Should Fail With    Failed to find a signing key
    Should Contain    ${stdout}    item=cryptoki: socket (key: pkcs11:token=token123)

    Execute Command    cmd=tedge config set c8y.device.key_uri "pkcs11:token=tedge;object=tedge"
    ${stdout}=    Tedge Reconnect Should Succeed
    Should Contain    ${stdout}    item=cryptoki: socket (key: pkcs11:token=tedge;object=tedge)

Connects to C8y using an RSA key
    [Documentation]    Test that we can connect to C8y using an RSA private keys of all sizes.
    [Setup]    Set tedge-p11-server Uri    value=${EMPTY}
    [Template]    Connect to C8y using new keypair
    type=rsa    bits=4096
    type=rsa    bits=3072
    type=rsa    bits=2048
    type=rsa    bits=1024

Connects to C8y supporting all TLS13 ECDSA signature algorithms
    [Documentation]    Check that we support all ECDSA sigschemes used in TLS1.3, i.e: ecdsa_secp256r1_sha256,
    ...    ecdsa_secp384r1_sha384, ecdsa_secp521r1_sha512.
    [Setup]    Set tedge-p11-server Uri    value=${EMPTY}
    [Template]    Connect to C8y using new keypair
    type=ecdsa    curve=secp256r1
    type=ecdsa    curve=secp384r1
    type=ecdsa    curve=secp521r1

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
Connect to C8y using new keypair
    [Documentation]    Generates a new key/cert pair, uploads it to c8y, and connects using it.
    [Arguments]    ${type}    # ecdsa or rsa
    ...    ${curve}=secp256r1    # if type == ECDSA, curve of the key - one of {secp256r1, secp384r1, secp521r1}
    ...    ${bits}=4096    # if type == RSA, length in bits of the RSA key - one of {1024, 2048, 3072, 4096}

    IF    '${type}' == 'ecdsa'
        VAR    ${object_name}=    ${type}-${curve}
        VAR    ${p11tool_args}=    --curve=${curve}
    ELSE IF    '${type}' == 'rsa'
        VAR    ${object_name}=    ${type}-${bits}
        VAR    ${p11tool_args}=    --bits=${bits}
    ELSE
        Fail    Wrong key type provided.
    END
    VAR    ${cert_path}=    /etc/tedge/device-certs/${object_name}.pem

    Execute Command
    ...    cmd=p11tool --set-pin=123456 --login --generate-privkey ${type} ${p11tool_args} --label ${object_name} "pkcs11:token=tedge"
    # we should probably generate certs signed by CA instead of uploading them
    Execute Command
    ...    cmd=GNUTLS_PIN=123456 certtool --generate-self-signed --template ${CERT_TEMPLATE} --outfile ${cert_path} --load-privkey "pkcs11:token=tedge;object=${object_name}"

    Execute Command    tedge config set c8y.device.cert_path ${cert_path}
    Execute Command    cmd=tedge config set c8y.device.key_uri "pkcs11:token=tedge;object=${object_name}"

    # upload (THIS STAYS ON C8Y AND ISN'T DELETED)
    Upload Currently Used Certificates To Cumulocity

    Tedge Reconnect Should Succeed

    Execute Command    tedge config unset c8y.device.cert_path
    Execute Command    tedge config unset c8y.device.key_uri

Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    # Allow the tedge user to access softhsm
    Execute Command    sudo usermod -a -G softhsm tedge
    Transfer To Device    ${CURDIR}/data/init_softhsm.sh    /usr/bin/
    Remove Existing Certificates

    # initialize the soft hsm and create a self-signed certificate
    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    Execute Command    sudo -u tedge /usr/bin/init_softhsm.sh --self-signed --device-id "${DEVICE_SN}" --pin 123456

    # configure tedge
    ${domain}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${domain}"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    Upload Currently Used Certificates To Cumulocity

    Set tedge-p11-server Uri    value=

Remove Existing Certificates
    Execute Command    cmd=rm -f "$(tedge config get device.key_path)" "$(tedge config get device.cert_path)"

Set tedge-p11-server Uri
    [Arguments]    ${value}
    Execute Command    tedge config set device.cryptoki.uri '${value}'
    Restart Service    tedge-p11-server

Tedge Reconnect Should Succeed
    ${stdout}=    Execute Command    tedge reconnect c8y
    RETURN    ${stdout}

Tedge Reconnect Should Fail With
    [Arguments]    ${error}
    ${stdout}    ${stderr}=    Execute Command    tedge reconnect c8y    exp_exit_code=!0    stdout=true    stderr=true
    Should Contain    ${stderr}    ${error}
    RETURN    ${stdout}

Upload Currently Used Certificates To Cumulocity
    Execute Command
    ...    cmd=sudo env C8Y_USER="${C8Y_CONFIG.username}" C8Y_PASSWORD="${C8Y_CONFIG.password}" tedge cert upload c8y
