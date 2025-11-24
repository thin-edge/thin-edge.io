*** Settings ***
Documentation       Tests for the `tedge cert create-key-hsm` command.

Resource            pkcs11_common.resource

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Variables ***
${KEY_URI}      ${EMPTY}
${TOKEN_URI}    pkcs11:token=create-key-token


*** Test Cases ***
Can create a private key on the PKCS11 token
    Execute Command    cmd=softhsm2-util --init-token --free --label create-key-token --pin=123456 --so-pin=123456

    ${output}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "${TOKEN_URI}"
    ...    exp_exit_code=!0
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Be Equal    ${output}    No matching objects found

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

Shows connected initialized tokens when token argument is not provided
    # setup multiple tokens
    Execute Command    cmd=softhsm2-util --init-token --free --label create-key-token1 --pin=123456 --so-pin=123456
    Execute Command    cmd=softhsm2-util --init-token --free --label create-key-token2 --pin=123456 --so-pin=123456

    # unset key_uri so there there's no hint where to generate the keypair
    Execute Command    cmd=tedge config unset device.key_uri
    ${stderr}=    Execute Command
    ...    cmd=tedge cert create-key-hsm --type ecdsa --label my-key
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=1
    Should Contain    ${stderr}    No token URL was provided for this operation; the available tokens are:
    Should Contain    ${stderr}    token=create-key-token1
    Should Contain    ${stderr}    token=create-key-token2

Can set key ID using --id flag
    ${output}=    Execute Command
    ...    cmd=tedge cert create-key-hsm --type ecdsa --label my-key --id 010203 "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${output}    id=%01%02%03

    ${output}=    Execute Command
    ...    cmd=tedge cert create-key-hsm --type ecdsa --label my-key --id 010203 "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=!0
    Should Contain    ${output}    Object with this id already exists on the token

Can provide PIN using --pin flag
    ${output}=    Execute Command
    ...    cmd=tedge cert create-key-hsm --label my-key --pin 000000 "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=!0
    Should Contain    ${output}    The specified PIN is incorrect

Saves public key to file using --outfile-pubkey flag
    ${output}=    Execute Command
    ...    cmd=tedge cert create-key-hsm --label my-key --outfile-pubkey pubkey.pem "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ${pubkey}=    Execute Command    cat pubkey.pem    strip=True
    Should Contain    ${output}    ${pubkey}


*** Keywords ***
Create private key
    [Arguments]    ${type}    ${label}    ${bits}=${EMPTY}    ${curve}=${EMPTY}    ${p11tool_keytype}=${EMPTY}
    # create the private key on token and write CSR to device.csr_path
    ${command}=    Set Variable    tedge cert create-key-hsm --label ${label} --type ${type} "${TOKEN_URI}"
    IF    $bits
        ${command}=    Set Variable    ${command} --bits ${bits}
    END
    IF    $curve
        ${command}=    Set Variable    ${command} --curve ${curve}
    END
    ${create_key_output}=    Execute Command    ${command}    strip=True    stderr=True    stdout=False

    # check if key is created
    ${output}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "${TOKEN_URI}"
    IF    $p11tool_keytype
        Should Contain    ${output}    Type: Private key (${p11tool_keytype})
    ELSE
        Should Contain    ${output}    Type: Private key
    END
    Should Contain    ${output}    Label: ${label}

    ${key_uri}=    Execute Command    tedge config get device.key_uri    strip=True
    Should Contain    ${create_key_output}    ${key_uri}

Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so

    # configure tedge
    ${domain}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${domain}"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket
