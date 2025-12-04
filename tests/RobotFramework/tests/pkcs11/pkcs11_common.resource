*** Settings ***
Documentation       Keywords used for testing thin-edge.io PKCS11 integration and tedge-p11-server.

Resource            ../resources/common.resource
Library             String
Library             Cumulocity
Library             ThinEdgeIO


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # should be set in setup of test suites

# if `False`, PKCS11 operations like creating keys, will use new tedge features, e.g. `tedge cert create-key-hsm`.
# if `True`, `p11tool` will be used instead
${PKCS11_USE_P11TOOL}    ${False}


*** Keywords ***
Create Self Signed Certificate
    [Arguments]    ${common_name}    ${label}    ${output_path}    ${pin}=123456
    Execute Command
    ...    cmd=printf 'dn = "CN=${common_name}, OU=Test Device, O=Thin Edge, ST=QLD, C=AU"\nca\n' | GNUTLS_PIN=${pin} certtool --generate-self-signed --template /dev/stdin --outfile "${output_path}" --load-privkey "pkcs11:token=tedge;object=${label}"

Connect to C8y using new ECDSA keypair
    [Documentation]    Connects to C8y with a newly generated keypair and a self-signed certificate.
    ...    The private key is saved on the token, and the self-signed certificate is registered with c8y.
    # if type == ECDSA, curve of the key - one of {secp256r1, secp384r1, secp521r1}
    [Arguments]    ${curve}=p256

    # We could alternatively use Cumulocity CA to start with a signed cert, but for testing certificate renewal, we want
    # to test both renewing a self-signed cert and a cert issued by C8y CA. When we start with self-signed cert, after
    # the first renewal we get a cert signed by CA, so we test all scenarios by just doing renew 2 times.
    ${label}=    Set up new PKCS11 ECDSA keypair    curve=${curve}

    ${cert_path}=    Set Variable    /etc/tedge/device-certs/${label}.pem
    Execute Command    cmd=tedge config set device.cert_path ${cert_path}

    Create Self Signed Certificate    common_name=${DEVICE_SN}    label=${label}    output_path=${cert_path}

    Execute Command
    ...    cmd=sudo env C8Y_USER="${C8Y_CONFIG.username}" C8Y_PASSWORD="${C8Y_CONFIG.password}" tedge cert upload c8y
    ThinEdgeIO.Register Certificate For Cleanup

    Tedge Reconnect Should Succeed

Connect to C8y using new RSA keypair
    [Documentation]    Connects to C8y with a newly generated keypair and a self-signed certificate.
    ...    The private key is saved on the token, and the self-signed certificate is registered with c8y.
    [Arguments]    ${bits}=4096    # length in bits of the RSA key - one of {1024, 2048, 3072, 4096}

    # We could alternatively use Cumulocity CA to start with a signed cert, but for testing certificate renewal, we want
    # to test both renewing a self-signed cert and a cert issued by C8y CA. When we start with self-signed cert, after
    # the first renewal we get a cert signed by CA, so we test all scenarios by just doing renew 2 times.

    # guarantee name of the object is unique even if multiple keys of the same type and bits/curve are generated
    ${label}=    Set up new PKCS11 RSA keypair    bits=${bits}

    ${cert_path}=    Set Variable    /etc/tedge/device-certs/${label}.pem
    Execute Command    cmd=tedge config set device.cert_path ${cert_path}

    Create Self Signed Certificate    common_name=${DEVICE_SN}    label=${label}    output_path=${cert_path}

    Execute Command
    ...    cmd=sudo env C8Y_USER="${C8Y_CONFIG.username}" C8Y_PASSWORD="${C8Y_CONFIG.password}" tedge cert upload c8y
    ThinEdgeIO.Register Certificate For Cleanup

    Tedge Reconnect Should Succeed

Set up new PKCS11 RSA keypair
    [Documentation]    Creates a new keypair on the PKCS11 token, configures thin-edge to use the new key
    [Arguments]    ${bits}=2048    # length in bits of the RSA key - one of {1024, 2048, 3072, 4096}
    ${identifier}=    String.Generate Random String
    ${label}=    Set Variable    rsa-${bits}-${identifier}

    IF    ${PKCS11_USE_P11TOOL}
        Execute Command
        ...    cmd=p11tool --set-pin=123456 --login --generate-privkey rsa --bits ${bits} --label ${label} "pkcs11:token=tedge"
    ELSE
        Execute Command
        ...    cmd=tedge cert create-key-hsm --type rsa --bits ${bits} --label "${label}" "pkcs11:token=tedge"
    END

    VAR    ${key_uri}=    pkcs11:token=tedge;object=${label}
    # warning: PKCS11 _uri variables contain ; so they need to be quoted!
    Execute Command    cmd=tedge config set device.key_uri "${key_uri}"
    RETURN    ${label}

Set up new PKCS11 ECDSA keypair
    [Documentation]    Creates a new keypair on the PKCS11 token, configures thin-edge to use the new key
    [Arguments]    ${curve}=p256    # curve of the key - one of {p256, p384}
    ${identifier}=    String.Generate Random String
    ${label}=    Set Variable    ecdsa-${curve}-${identifier}

    IF    ${PKCS11_USE_P11TOOL}
        Execute Command
        ...    cmd=p11tool --set-pin=123456 --login --generate-privkey ecdsa --curve sec${curve}r1 --label ${label} "pkcs11:token=tedge"
    ELSE
        Execute Command
        ...    cmd=tedge cert create-key-hsm --type ecdsa --curve ${curve} --label "${label}" "pkcs11:token=tedge"
    END

    VAR    ${key_uri}=    pkcs11:token=tedge;object=${label}
    # warning: PKCS11 _uri variables contain ; so they need to be quoted!
    Execute Command    cmd=tedge config set device.key_uri "${key_uri}"
    RETURN    ${label}

Set tedge-p11-server Uri
    [Arguments]    ${value}
    Execute Command    tedge config set device.cryptoki.uri '${value}'
    Restart Service    tedge-p11-server

Unset tedge-p11-server Uri
    Execute Command    tedge config unset device.cryptoki.uri
    Restart Service    tedge-p11-server

Tedge Reconnect Should Succeed
    ${stderr}=    Execute Command    tedge reconnect c8y    stdout=false    stderr=true
    RETURN    ${stderr}

Tedge Reconnect Should Fail With
    [Arguments]    ${error}
    ${stderr}=    Command Should Fail With    tedge reconnect c8y    ${error}
    RETURN    ${stderr}

Command Should Fail With
    [Arguments]    ${command}    ${error}
    ${stderr}=    Execute Command    ${command}    exp_exit_code=!0    stdout=false    stderr=true
    Should Contain    ${stderr}    ${error}
    RETURN    ${stderr}

Install tedge-p11-server
    [Arguments]    ${version}=""

    # this doesn't install anything but adds cloudsmith repo to apt
    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.deb.sh' | sudo -E bash

    IF    $version
        VAR    ${package}=    tedge-p11-server=${version}
    ELSE
        VAR    ${package}=    tedge-p11-server
    END
    Execute Command    cmd=apt-get install -y --allow-downgrades ${package}

    IF    $version
        ${stdout}=    Execute Command    tedge-p11-server -V    strip=True
        Should Be Equal    ${stdout}    tedge-p11-server ${version}
    END
