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

Initialize a specific slot by URI
    [Documentation]    The slot to initialize can be selected explicitly by passing the PKCS #11 URI
    ...    printed by `tedge hsm list-tokens`. An uninitialized slot has no label or serial, so it is
    ...    addressed by its model and slot id.
    ${tokens}=    Execute Command    cmd=tedge hsm list-tokens    strip=True
    ${slot_uris}=    Get Regexp Matches    ${tokens}    URI:\\s+(pkcs11:\\S*slot-id=\\d+)    1
    Should Not Be Empty    ${slot_uris}    msg=No uninitialized slot was listed:\n${tokens}
    VAR    ${slot_uri}=    ${slot_uris}[0]

    ${uri}=    Execute Command
    ...    cmd=tedge hsm init --label slot-token "${slot_uri}"
    ...    strip=True
    ...    stdout=True
    ...    stderr=False
    Should Start With    ${uri}    pkcs11:
    Should Contain    ${uri}    token=slot-token

    # the new token is usable, and the tokens initialized earlier are left untouched
    # (note: SoftHSM2 reassigns the slot id once a token is initialized in it)
    ${tokens}=    Execute Command    cmd=p11tool --list-tokens
    Should Contain    ${tokens}    token=slot-token
    Should Contain    ${tokens}    token=manual-token

    # A slot URI is strict: initializing the token renumbered the slots, so the slot URI used
    # above no longer matches anything and re-running the same command fails rather than
    # guessing another slot
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm init --label slot-token "${slot_uri}"
    ...    exp_exit_code=!0
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    No token matching the given URI was found

    # Re-runnable scripts should omit the URI: auto-discovery finds the token by its label,
    # making the command idempotent
    ${uri2}=    Execute Command
    ...    cmd=tedge hsm init --label slot-token
    ...    strip=True
    ...    stdout=True
    ...    stderr=False
    Should Be Equal    ${uri}    ${uri2}

    # Even with a valid URI of a free slot, an already-satisfied label is idempotent: the existing
    # token is returned and no duplicate label is created on the selected slot
    ${tokens}=    Execute Command    cmd=tedge hsm list-tokens    strip=True
    ${free_uris}=    Get Regexp Matches    ${tokens}    URI:\\s+(pkcs11:\\S*slot-id=\\d+)    1
    Should Not Be Empty    ${free_uris}    msg=No uninitialized slot was listed:\n${tokens}
    ${uri3}=    Execute Command
    ...    cmd=tedge hsm init --label slot-token "${free_uris}[0]"
    ...    strip=True
    ...    stdout=True
    ...    stderr=False
    Should Be Equal    ${uri}    ${uri3}

    ${tokens}=    Execute Command    cmd=p11tool --list-tokens
    ${count}=    Get Count    ${tokens}    token=slot-token
    Should Be Equal As Integers    ${count}    1

Delete a key with hsm delete-key
    [Documentation]    delete-key destroys the key objects matching a label, asks for confirmation,
    ...    refuses to delete the configured signing key, and is idempotent.
    Execute Command    cmd=tedge hsm create-key --type ecdsa --label doomed-key "pkcs11:token=tedge"

    # without --force and no confirmation on stdin, nothing is deleted
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm delete-key --label doomed-key "pkcs11:token=tedge" </dev/null
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    Aborted; no keys were deleted.
    ${privkeys}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "pkcs11:token=tedge"
    Should Contain    ${privkeys}    Label: doomed-key

    # deleting the key referenced by device.key_uri is refused
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm delete-key --label auto-key
    ...    exp_exit_code=!0
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    Refusing to delete
    Should Contain    ${stderr}    device.key_uri

    # confirming the prompt deletes the private and public key objects
    ${stderr}=    Execute Command
    ...    cmd=echo y | tedge hsm delete-key --label doomed-key "pkcs11:token=tedge"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    Deleted
    ${privkeys}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "pkcs11:token=tedge"
    Should Not Contain    ${privkeys}    Label: doomed-key

    # deleting an absent key is a no-op success
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm delete-key --force --label doomed-key "pkcs11:token=tedge"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    No matching key was found; nothing to delete.

Change the user PIN with hsm change-pin
    [Documentation]    change-pin changes the user PIN of the selected token and keeps
    ...    device.cryptoki.pin in sync. With several initialized tokens, the token must be
    ...    selected explicitly by URI.
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm change-pin --new-pin 654321 --current-pin 123456
    ...    exp_exit_code=!0
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    Found multiple initialized tokens

    ${stderr}=    Execute Command
    ...    cmd=tedge hsm change-pin --new-pin 654321 --current-pin 123456 "pkcs11:token=tedge"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    was changed
    Should Contain    ${stderr}    device.cryptoki.pin

    # the new PIN is required to log in to the token
    ${privkeys}=    Execute Command
    ...    cmd=p11tool --login --set-pin=654321 --list-privkeys "pkcs11:token=tedge"
    Should Contain    ${privkeys}    Label: auto-key

    # the configured PIN was kept in sync
    ${pin}=    Execute Command    tedge config get device.cryptoki.pin    strip=True
    Should Be Equal    ${pin}    654321

Reset the user PIN using the Security Officer PIN
    [Documentation]    change-pin --reset recovers a token whose user PIN is unknown, using the SO
    ...    PIN. The token was auto-initialized without --so-pin, so its SO PIN defaulted to the
    ...    user PIN it was created with (123456).
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm change-pin --reset --new-pin 123456 --so-pin 123456 "pkcs11:token=tedge"
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    was reset

    ${privkeys}=    Execute Command
    ...    cmd=p11tool --login --set-pin=123456 --list-privkeys "pkcs11:token=tedge"
    Should Contain    ${privkeys}    Label: auto-key

    ${pin}=    Execute Command    tedge config get device.cryptoki.pin    strip=True
    Should Be Equal    ${pin}    123456

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
