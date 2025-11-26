*** Settings ***
Documentation       This test suite runs the tests with tedge-p11-server pinned to a fixed version to ensure that new
...                 versions of thin-edge remain backwards compatible with tedge-p11-server's binary communication protocol. The
...                 scope of this test is limited to tedge-p11-server's initial feature set and will generally not be expanded.

Resource            pkcs11_common.resource

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki    compatibility


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     1.5.1


*** Test Cases ***
# the test cases are basically copy-pasted from private_key_storage.robot, as the purpose of this suite is to run the
# exact same tests with a slightly different setup. It would be easiest if we could import the test cases themselves
# from another test suite, but this isn't possible. So we extract reusable keywords into a resource file, but test cases
# remain duplicated.
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
    [Template]    Connect to C8y using new RSA keypair
    bits=4096
    bits=3072
    bits=2048
    # bits=1024    # RSA 1024 is considered to be insecure is not supported when using the Cumulocity Certificate Authority feature

Connects to C8y supporting all TLS13 ECDSA signature algorithms
    [Documentation]    Check that we support all ECDSA sigschemes used in TLS1.3, i.e: ecdsa_secp256r1_sha256,
    ...    ecdsa_secp384r1_sha384, ecdsa_secp521r1_sha512.
    [Setup]    Unset tedge-p11-server Uri
    [Template]    Connect to C8y using new ECDSA keypair
    curve=secp256r1

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
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    # this doesn't install anything but adds cloudsmith repo to apt
    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.deb.sh' | sudo -E bash
    Execute Command    cmd=apt-get install -y --allow-downgrades tedge-p11-server=${TEDGE_P11_SERVER_VERSION}
    ${stdout}=    Execute Command    tedge-p11-server -V    strip=True
    Should Be Equal    ${stdout}    tedge-p11-server ${TEDGE_P11_SERVER_VERSION}

    # Allow the tedge user to access softhsm
    Execute Command    sudo usermod -a -G softhsm tedge
    Transfer To Device    ${CURDIR}/data/init_softhsm.sh    /usr/bin/

    # initialize the soft hsm and create a certificate signing request
    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    Execute Command    sudo -u tedge /usr/bin/init_softhsm.sh --device-id "${DEVICE_SN}" --pin 123456

    # configure tedge
    Set Cumulocity URLs
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    ${csr_path}=    Execute Command    cmd=tedge config get device.csr_path    strip=${True}
    Register Device With Cumulocity CA    ${DEVICE_SN}    csr_path=${csr_path}

    Unset tedge-p11-server Uri

Connect to C8y using new ECDSA keypair
    [Documentation]    Connects to C8y with a newly generated keypair and a self-signed certificate.
    ...    The private key is saved on the token, and the self-signed certificate is registered with c8y.
    [Arguments]    ${curve}=secp256r1
    ${label}=    Set up new PKCS11 ECDSA keypair    curve=${curve}

    ${cert_path}=    Set Variable    /etc/tedge/device-certs/${label}.pem
    Execute Command    cmd=tedge config set device.cert_path ${cert_path}

    Create Self Signed Certificate    common_name=${DEVICE_SN}    label=${label}    output_path=${cert_path}
    Set tedge-p11-server Uri    value=pkcs11:token=tedge;object=${label}

    Execute Command
    ...    cmd=sudo env C8Y_USER="${C8Y_CONFIG.username}" C8Y_PASSWORD="${C8Y_CONFIG.password}" tedge cert upload c8y
    ThinEdgeIO.Register Certificate For Cleanup

    Tedge Reconnect Should Succeed

Connect to C8y using new RSA keypair
    [Documentation]    Connects to C8y with a newly generated keypair and a self-signed certificate.
    ...    The private key is saved on the token, and the self-signed certificate is registered with c8y.
    [Arguments]    ${bits}=4096    # length in bits of the RSA key - one of {1024, 2048, 3072, 4096}
    ${label}=    Set up new PKCS11 RSA keypair    bits=${bits}

    ${cert_path}=    Set Variable    /etc/tedge/device-certs/${label}.pem
    Execute Command    cmd=tedge config set device.cert_path ${cert_path}

    Create Self Signed Certificate    common_name=${DEVICE_SN}    label=${label}    output_path=${cert_path}
    Set tedge-p11-server Uri    value=pkcs11:token=tedge;object=${label}

    Execute Command
    ...    cmd=sudo env C8Y_USER="${C8Y_CONFIG.username}" C8Y_PASSWORD="${C8Y_CONFIG.password}" tedge cert upload c8y
    ThinEdgeIO.Register Certificate For Cleanup

    Tedge Reconnect Should Succeed

Set up new PKCS11 RSA keypair
    [Documentation]    Creates a new keypair on the PKCS11 token, configures thin-edge to use the new key
    [Arguments]    ${bits}=2048    # length in bits of the RSA key - one of {1024, 2048, 3072, 4096}
    ${identifier}=    String.Generate Random String
    ${label}=    Set Variable    rsa-${bits}-${identifier}
    Execute Command
    ...    cmd=p11tool --set-pin=123456 --login --generate-privkey rsa --bits=${bits} --label "${label}" --outfile "/etc/tedge/hsm/${label}.pub" "pkcs11:token=tedge"
    RETURN    ${label}

Set up new PKCS11 ECDSA keypair
    [Documentation]    Creates a new keypair on the PKCS11 token, configures thin-edge to use the new key
    [Arguments]    ${curve}=p256    # curve of the key - one of {p256, p384}
    ${identifier}=    String.Generate Random String
    ${label}=    Set Variable    ecdsa-${curve}-${identifier}
    Execute Command
    ...    cmd=p11tool --set-pin=123456 --login --generate-privkey ECDSA --curve ${curve} --label "${label}" --outfile "/etc/tedge/hsm/${label}.pub" "pkcs11:token=tedge"
    RETURN    ${label}
