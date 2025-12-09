*** Settings ***
Resource    ../pkcs11_common.resource


*** Keywords ***
Use PKCS11 key to renew the public certificate
    [Documentation]    Test that `tedge cert renew c8y` works with all supported keys. We do renew 2 times to see if we
    ...    can renew both a self-signed certificate and a certificate signed by C8y CA.
    [Arguments]    ${error}
    [Setup]    Unset tedge-p11-server Uri

    Test tedge cert renew    type=ecdsa    error=${error}    curve=p256
    Test tedge cert renew    type=ecdsa    error=${error}    curve=p384

    # renewal isn't supported for secp521r1 because rcgen doesn't support it
    # https://github.com/rustls/rcgen/issues/60

    Test tedge cert renew    type=rsa    error=${error}    bits=2048
    Test tedge cert renew    type=rsa    error=${error}    bits=3072
    Test tedge cert renew    type=rsa    error=${error}    bits=4096

Test tedge cert renew
    [Arguments]    ${type}    ${error}    ${bits}=${EMPTY}    ${curve}=${EMPTY}
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

    Execute Command    systemctl stop tedge-p11-server tedge-p11-server.socket
    Command Should Fail With
    ...    tedge cert renew c8y
    ...    error=Failed to connect to tedge-p11-server UNIX socket at '/run/tedge-p11-server/tedge-p11-server.sock'

    Execute Command    systemctl start tedge-p11-server.socket

    Execute Command    cmd=tedge config set c8y.device.key_uri pkcs11:object=nonexistent_key
    Command Should Fail With    tedge cert renew c8y    ${error}
    Execute Command    cmd=tedge config unset c8y.device.key_uri

Renew the certificate using different keypair
    [Documentation]    Starting with an initial trusted certificate, replace the keypair and renew the certificate.
    Connect to C8y using new ECDSA keypair
    Set up new PKCS11 ECDSA keypair
    Execute Command    tedge cert renew c8y
    ${stdout}=    Tedge Reconnect Should Succeed
    Should Contain    ${stdout}    The new certificate is now the active certificate
