*** Settings ***
Documentation       Keywords for testing `tedge hsm init` and automatic token initialization.

Resource            ../pkcs11_common.resource


*** Keywords ***
Auto initialize token when creating a key
    [Documentation]    On a device with no initialized token, `tedge hsm create-key` should
    ...    initialize an uninitialized slot automatically before creating the key.

    # No initialized token exists yet and no key_uri is configured to hint a token.
    Execute Command    cmd=tedge config unset device.key_uri

    ${output}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label auto-key
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${output}    No initialized token was found
    Should Contain    ${output}    Initialized a new token
    Should Contain    ${output}    New keypair was successfully created.

    # The default token label is `tedge`, and device.key_uri is updated to the new key.
    ${key_uri}=    Execute Command    tedge config get device.key_uri    strip=True
    Should Contain    ${key_uri}    token=tedge

    ${privkeys}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "pkcs11:token=tedge"
    Should Contain    ${privkeys}    Label: auto-key

create-key reuses an existing key by default
    [Documentation]    Re-running create-key with the same label reuses the existing key instead
    ...    of creating a duplicate, so the command is idempotent.
    ${output}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label auto-key "pkcs11:token=tedge"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${output}    reusing it

    # still exactly one private key with this label
    ${privkeys}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "pkcs11:token=tedge"
    ${count}=    Get Count    ${privkeys}    Label: auto-key
    Should Be Equal As Integers    ${count}    1

create-key --force-new creates a new key
    [Documentation]    With --force-new a new key is generated even when one with the same label
    ...    already exists.
    ${output}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label auto-key --force-new "pkcs11:token=tedge"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${output}    New keypair was successfully created.

    # now two private keys share the label
    ${privkeys}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "pkcs11:token=tedge"
    ${count}=    Get Count    ${privkeys}    Label: auto-key
    Should Be Equal As Integers    ${count}    2

Initialize a token with hsm init
    [Documentation]    `tedge hsm init` initializes the free/uninitialized slot and
    ...    returns the resulting token URI on stdout so it can be captured in scripts.
    ${uri}=    Execute Command
    ...    cmd=tedge hsm init --label manual-token
    ...    strip=True
    ...    stdout=True
    ...    stderr=False
    # stdout must be exactly the token URI (nothing else), so `URI=$(tedge hsm init)` works
    Should Start With    ${uri}    pkcs11:
    Should Contain    ${uri}    token=manual-token

    ${tokens}=    Execute Command    cmd=p11tool --list-tokens
    Should Contain    ${tokens}    token=manual-token

    # The returned URI can be fed straight into create-key
    Execute Command    cmd=tedge hsm create-key --type ecdsa --label script-key "${uri}"
    ${privkeys}=    Execute Command    cmd=p11tool --login --set-pin=123456 --list-privkeys "${uri}"
    Should Contain    ${privkeys}    Label: script-key

Initializing an existing token is idempotent
    [Documentation]    Re-initializing a token with the same label leaves it untouched and returns
    ...    its URI (on stdout) without error.
    ${uri}=    Execute Command
    ...    cmd=tedge hsm init --label manual-token
    ...    strip=True
    ...    stdout=True
    ...    stderr=False
    Should Start With    ${uri}    pkcs11:
    Should Contain    ${uri}    token=manual-token

    ${tokens}=    Execute Command    cmd=p11tool --list-tokens
    Should Contain    ${tokens}    token=manual-token

    # Idempotent init must not erase the key created earlier on this token
    ${privkeys}=    Execute Command    cmd=p11tool --login --set-pin=123456 --list-privkeys "${uri}"
    Should Contain    ${privkeys}    Label: script-key

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
