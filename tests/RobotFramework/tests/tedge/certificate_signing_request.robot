*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Teardown       Get Logs

Test Tags           theme:cli


*** Test Cases ***
Generate CSR using the device-id from an existing certificate and private key
    [Setup]    Setup With Self-Signed Certificate
    ThinEdgeIO.File Should Exist    /etc/tedge/device-certs/tedge-certificate.pem
    ThinEdgeIO.File Should Exist    /etc/tedge/device-certs/tedge-private-key.pem

    ${hash_before_cert}=    Execute Command    md5sum /etc/tedge/device-certs/tedge-certificate.pem
    ${hash_before_private_key}=    Execute Command    md5sum /etc/tedge/device-certs/tedge-private-key.pem

    Execute Command    sudo tedge cert create-csr

    ${output_cert_subject}=    Execute Command
    ...    openssl x509 -noout -subject -in /etc/tedge/device-certs/tedge-certificate.pem
    ${output_csr_subject}=    Execute Command
    ...    openssl req -noout -subject -in /etc/tedge/device-certs/tedge.csr
    Should Be Equal    ${output_cert_subject}    ${output_csr_subject}

    ${output_private_key_md5}=    Execute Command
    ...    openssl pkey -in /etc/tedge/device-certs/tedge-private-key.pem -pubout | openssl md5
    ${output_csr_md5}=    Execute Command
    ...    openssl req -in /etc/tedge/device-certs/tedge.csr -pubkey -noout | openssl md5
    Should Be Equal    ${output_private_key_md5}    ${output_csr_md5}

    ${hash_after_cert}=    Execute Command    md5sum /etc/tedge/device-certs/tedge-certificate.pem
    ${hash_after_private_key}=    Execute Command    md5sum /etc/tedge/device-certs/tedge-private-key.pem
    Should Be Equal    ${hash_before_cert}    ${hash_after_cert}
    Should Be Equal    ${hash_before_private_key}    ${hash_after_private_key}

Generate CSR without an existing certificate and private key
    [Setup]    Setup Without Certificate
    File Should Not Exist    /etc/tedge/device-certs/tedge-certificate.pem
    File Should Not Exist    /etc/tedge/device-certs/tedge-private-key.pem

    Execute Command    sudo tedge cert create-csr --device-id test-user

    ${output_csr_subject}=    Execute Command
    ...    openssl req -noout -subject -in /etc/tedge/device-certs/tedge.csr | tr -d ' '
    Should Contain    ${output_csr_subject}    subject=CN=test-user

    ${output_private_key_md5}=    Execute Command
    ...    openssl pkey -in /etc/tedge/device-certs/tedge-private-key.pem -pubout | openssl md5
    ${output_csr_md5}=    Execute Command
    ...    openssl req -in /etc/tedge/device-certs/tedge.csr -pubkey -noout | openssl md5
    Should Be Equal    ${output_private_key_md5}    ${output_csr_md5}

Generate CSR using the device-id from an existing certificate and private key of cloud profile
    [Tags]    \#3315
    [Setup]    Setup With Self-Signed Certificate

    ${second_device_sn}=    Catenate    SEPARATOR=_    ${DEVICE_SN}    second
    Execute Command
    ...    tedge config set c8y.device.cert_path --profile second /etc/tedge/device-certs/tedge@second-certificate.pem
    Execute Command
    ...    tedge config set c8y.device.key_path --profile second /etc/tedge/device-certs/tedge@second-key.pem
    Execute Command    tedge cert create --device-id ${second_device_sn} c8y --profile second

    ${hash_before_cert}=    Execute Command    md5sum /etc/tedge/device-certs/tedge@second-certificate.pem
    ${hash_before_private_key}=    Execute Command    md5sum /etc/tedge/device-certs/tedge@second-key.pem

    Execute Command    sudo tedge cert create-csr c8y --profile second

    ${output_cert_subject}=    Execute Command
    ...    openssl x509 -noout -subject -in /etc/tedge/device-certs/tedge@second-certificate.pem
    ${output_csr_subject}=    Execute Command
    ...    openssl req -noout -subject -in /etc/tedge/device-certs/tedge.csr
    Should Be Equal    ${output_cert_subject}    ${output_csr_subject}

    ${output_private_key_md5}=    Execute Command
    ...    openssl pkey -in /etc/tedge/device-certs/tedge@second-key.pem -pubout | openssl md5
    ${output_csr_md5}=    Execute Command
    ...    openssl req -in /etc/tedge/device-certs/tedge.csr -pubkey -noout | openssl md5
    Should Be Equal    ${output_private_key_md5}    ${output_csr_md5}

    ${hash_after_cert}=    Execute Command    md5sum /etc/tedge/device-certs/tedge@second-certificate.pem
    ${hash_after_private_key}=    Execute Command    md5sum /etc/tedge/device-certs/tedge@second-key.pem
    Should Be Equal    ${hash_before_cert}    ${hash_after_cert}
    Should Be Equal    ${hash_before_private_key}    ${hash_after_private_key}

Generate CSR using the CSR path from tedge config
    [Setup]    Setup Without Certificate
    File Should Not Exist    /etc/tedge/device-certs/tedge-certificate.pem
    File Should Not Exist    /etc/tedge/device-certs/tedge-private-key.pem

    Execute Command
    ...    tedge config set c8y.device.csr_path /etc/tedge/device-certs/example_dev_test0001-test1.csr

    Execute Command    sudo tedge cert create-csr c8y --device-id test-user

    ${output_csr_subject}=    Execute Command
    ...    openssl req -noout -subject -in /etc/tedge/device-certs/example_dev_test0001-test1.csr | tr -d ' '
    Should Contain    ${output_csr_subject}    subject=CN=test-user

    ${output_private_key_md5}=    Execute Command
    ...    openssl pkey -in /etc/tedge/device-certs/tedge-private-key.pem -pubout | openssl md5
    ${output_csr_md5}=    Execute Command
    ...    openssl req -in /etc/tedge/device-certs/example_dev_test0001-test1.csr -pubkey -noout | openssl md5
    Should Be Equal    ${output_private_key_md5}    ${output_csr_md5}

Generate CSR reusing RSA private key
    [Setup]    Setup Without Certificate
    File Should Not Exist    /etc/tedge/device-certs/device-csr.pem
    File Should Not Exist    /etc/tedge/device-certs/device-key.pem
    Execute Command
    ...    tedge config set device.csr_path /etc/tedge/device-certs/device-csr.pem
    Execute Command
    ...    tedge config set device.key_path /etc/tedge/device-certs/device-key.pem
    Execute Command
    ...    openssl genrsa -out /etc/tedge/device-certs/device-key.pem
    Execute Command
    ...    tedge cert create-csr --device-id RSA-device
    File Should Exist    /etc/tedge/device-certs/device-csr.pem
    ${output_csr_subject}=    Execute Command
    ...    openssl req -noout -subject -in /etc/tedge/device-certs/device-csr.pem | tr -d ' '
    Should Contain    ${output_csr_subject}    subject=CN=RSA-device

Generate CSR reusing EC private key
    [Setup]    Setup Without Certificate
    File Should Not Exist    /etc/tedge/device-certs/device-csr.pem
    File Should Not Exist    /etc/tedge/device-certs/device-key.pem
    Execute Command
    ...    tedge config set device.csr_path /etc/tedge/device-certs/device-csr.pem
    Execute Command
    ...    tedge config set device.key_path /etc/tedge/device-certs/device-key.pem
    Execute Command
    ...    sudo openssl ecparam -genkey -out "/etc/tedge/device-certs/device-key.tmp" -name secp256r1
    Execute Command
    ...    sudo openssl pkcs8 -topk8 -nocrypt -in "/etc/tedge/device-certs/device-key.tmp" -out "/etc/tedge/device-certs/device-key.pem"
    Execute Command
    ...    tedge cert create-csr --device-id EC-device
    File Should Exist    /etc/tedge/device-certs/device-csr.pem
    ${output_csr_subject}=    Execute Command
    ...    openssl req -noout -subject -in /etc/tedge/device-certs/device-csr.pem | tr -d ' '
    Should Contain    ${output_csr_subject}    subject=CN=EC-device

Generate CSR using a relative output path
    [Setup]    Setup With Self-Signed Certificate
    Execute Command
    ...    sudo tedge cert create-csr c8y --device-id test-user --output-path ./local.csr && [ -f ./local.csr ]


*** Keywords ***
Setup With Self-Signed Certificate
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --cert-method selfsigned

Setup Without Certificate
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --install --no-bootstrap --no-connect
