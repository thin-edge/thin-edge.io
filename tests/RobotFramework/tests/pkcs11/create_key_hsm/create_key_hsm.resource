*** Settings ***
Documentation       Tests for the `tedge hsm create-key` command.

Resource            ../pkcs11_common.resource


*** Variables ***
${KEY_URI}      ${EMPTY}
${TOKEN_URI}    pkcs11:token=create-key-token


*** Keywords ***
Create a private key on the PKCS11 token
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

Show connected initialized tokens when token argument is not provided
    # setup multiple tokens
    Execute Command    cmd=softhsm2-util --init-token --free --label create-key-token1 --pin=123456 --so-pin=123456
    Execute Command    cmd=softhsm2-util --init-token --free --label create-key-token2 --pin=123456 --so-pin=123456

    # unset key_uri so there there's no hint where to generate the keypair
    Execute Command    cmd=tedge config unset device.key_uri
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label my-key
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=1
    Should Contain    ${stderr}    No token URL was provided for this operation; the available tokens are:
    Should Contain    ${stderr}    token=create-key-token1
    Should Contain    ${stderr}    token=create-key-token2

Set key ID using --id flag
    ${output}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label my-key --id 010203 "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${output}    id=%01%02%03

    ${output}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label my-key --id 010203 "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=0
    Should Contain    ${output}    A key labelled 'my-key' already exists on the token

Provide PIN using --pin flag
    # Use a label that does not yet exist so the command takes the create path (which honours the
    # --pin flag) instead of idempotently reusing an existing key with the configured PIN.
    ${output}=    Execute Command
    ...    cmd=tedge hsm create-key --label wrong-pin-key --pin 000000 "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=!0
    Should Contain    ${output}    The specified PIN is incorrect

Save public key to file using --outfile-pubkey flag
    ${output}=    Execute Command
    ...    cmd=tedge hsm create-key --label my-key --outfile-pubkey pubkey.pem "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ${pubkey}=    Execute Command    cat pubkey.pem    strip=True
    Should Contain    ${output}    ${pubkey}

Deprecated cert create-key-hsm alias still works
    [Documentation]    `tedge cert create-key-hsm` is kept as a hidden deprecated alias for
    ...    `tedge hsm create-key`, so existing scripts keep working.
    ${output}=    Execute Command
    ...    cmd=tedge cert create-key-hsm --type ecdsa --label deprecated-alias-key "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${output}    deprecated
    Should Contain    ${output}    New keypair was successfully created.

    ${privkeys}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "${TOKEN_URI}"
    Should Contain    ${privkeys}    Label: deprecated-alias-key

Create private key
    [Arguments]    ${type}    ${label}    ${bits}=${EMPTY}    ${curve}=${EMPTY}    ${p11tool_keytype}=${EMPTY}
    # create the private key on token and write CSR to device.csr_path
    ${command}=    Set Variable    tedge hsm create-key --label ${label} --type ${type} "${TOKEN_URI}"
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

    # the URI of the created key is reported. Note that only the first created key is saved to
    # device.key_uri: subsequent invocations leave the existing working selection unchanged.
    Should Contain    ${create_key_output}    object=${label}

List keys filtered by label or id
    [Documentation]    `tedge hsm list-keys` can narrow the listing to keys with a given label or
    ...    id, either via the --label/--id flags or via `object=`/`id=` URI attributes.
    ${output}=    Execute Command    cmd=tedge hsm list-keys --label ec-256 "${TOKEN_URI}"    strip=True
    Should Contain    ${output}    Label: ec-256
    Should Not Contain    ${output}    Label: rsa-2048

    # equivalent filtering via the object= URI attribute
    ${output}=    Execute Command
    ...    cmd=tedge hsm list-keys "pkcs11:token=create-key-token;object=ec-256"
    ...    strip=True
    Should Contain    ${output}    Label: ec-256
    Should Not Contain    ${output}    Label: rsa-2048

    # filter by key id (my-key was created with --id 010203)
    ${output}=    Execute Command    cmd=tedge hsm list-keys --id 010203 "${TOKEN_URI}"    strip=True
    Should Contain    ${output}    Label: my-key
    Should Not Contain    ${output}    Label: ec-256

    # a filter matching nothing reports it
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm list-keys --label does-not-exist "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    No keys matching the given filters were found on the token.

    # giving the same filter via flag and URI attribute is ambiguous and rejected
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm list-keys --label x "pkcs11:token=create-key-token;object=y"
    ...    exp_exit_code=!0
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    use only one

Do not overwrite a key_uri that points to another existing key
    [Documentation]    When device.key_uri already selects a key that exists on the token,
    ...    creating another key must not silently replace the selection; the command to switch
    ...    is printed instead.
    ${before}=    Execute Command    tedge config get device.key_uri    strip=True

    ${stderr}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label protection-test-key "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    was left unchanged, as it points to another existing key
    Should Contain    ${stderr}    tedge config set device.key_uri

    ${after}=    Execute Command    tedge config get device.key_uri    strip=True
    Should Be Equal    ${before}    ${after}

Update key_uri when it points to a key that no longer exists
    [Documentation]    A key_uri pointing at a key that is not present on the token selects
    ...    nothing, so it is repaired to point at the newly created key (with a note).
    Execute Command
    ...    cmd=tedge config set device.key_uri "pkcs11:token=create-key-token;object=deleted-key"

    ${stderr}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label replacement-key "${TOKEN_URI}"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    pointed to a key that no longer exists
    Should Contain    ${stderr}    was updated with the newly created key's URI

    ${after}=    Execute Command    tedge config get device.key_uri    strip=True
    Should Contain    ${after}    object=replacement-key

Custom Setup
    [Arguments]    ${tedge_p11_server_version}
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}

    Execute Command    tedge config set device.cryptoki.pin 123456
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so

    # configure tedge
    ${domain}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${domain}"
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.mode socket

    Install tedge-p11-server    ${tedge_p11_server_version}
