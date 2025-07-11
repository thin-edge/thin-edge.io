*** Settings ***
Documentation       Keywords used for testing thin-edge.io PKCS11 integration and tedge-p11-server.

Resource            ../resources/common.resource
Library             String
Library             Cumulocity
Library             ThinEdgeIO


*** Variables ***
${DEVICE_SN}        ${EMPTY}    # should be set in setup of test suites
${CERT_TEMPLATE}    /etc/tedge/hsm/cert.template    # created by init_softhsm script


*** Keywords ***
Connect to C8y using new keypair
    [Documentation]    Connects to C8y with a newly generated keypair and a self-signed certificate.
    ...    The private key is saved on the token, and the self-signed certificate is registered with c8y.
    [Arguments]    ${type}    # ecdsa or rsa
    ...    ${curve}=secp256r1    # if type == ECDSA, curve of the key - one of {secp256r1, secp384r1, secp521r1}
    ...    ${bits}=4096    # if type == RSA, length in bits of the RSA key - one of {1024, 2048, 3072, 4096}

    # We could alternatively use Cumulocity CA to start with a signed cert, but for testing certificate renewal, we want
    # to test both renewing a self-signed cert and a cert issued by C8y CA. When we start with self-signed cert, after
    # the first renewal we get a cert signed by CA, so we test all scenarios by just doing renew 2 times.

    IF    '${type}' == 'ecdsa'
        VAR    ${object_name}=    ${type}-${curve}
        VAR    ${p11tool_args}=    --curve=${curve}
    ELSE IF    '${type}' == 'rsa'
        VAR    ${object_name}=    ${type}-${bits}
        VAR    ${p11tool_args}=    --bits=${bits}
    ELSE
        Fail    Wrong key type provided.
    END

    # guarantee name of the object is unique even if multiple keys of the same type and bits/curve are generated
    ${identifier}=    String.Generate Random String
    VAR    ${object_name}=    ${object_name}-${identifier}

    VAR    ${cert_path}=    /etc/tedge/device-certs/${object_name}.csr
    Execute Command    cmd=tedge config set device.cert_path ${cert_path}

    Execute Command
    ...    cmd=p11tool --set-pin=123456 --login --generate-privkey ${type} ${p11tool_args} --label ${object_name} "pkcs11:token=tedge"
    Execute Command
    ...    cmd=GNUTLS_PIN=123456 certtool --generate-self-signed --template "${CERT_TEMPLATE}" --outfile "${cert_path}" --load-privkey "pkcs11:token=tedge;object=${object_name}"

    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=tedge;object=${object_name}"

    Execute Command
    ...    cmd=sudo env C8Y_USER="${C8Y_CONFIG.username}" C8Y_PASSWORD="${C8Y_CONFIG.password}" tedge cert upload c8y
    ThinEdgeIO.Register Certificate For Cleanup

    Tedge Reconnect Should Succeed

Set tedge-p11-server Uri
    [Arguments]    ${value}
    Execute Command    tedge config set device.cryptoki.uri '${value}'
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

Register Device With Cumulocity CA
    [Documentation]    Registers a new certificate with Cumulocity CA and places it under `c8y.device.cert_path`
    [Arguments]    ${csr_path}
    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}
    Execute Command
    ...    cmd=tedge cert download c8y --csr-path "${csr_path}" --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 60s
