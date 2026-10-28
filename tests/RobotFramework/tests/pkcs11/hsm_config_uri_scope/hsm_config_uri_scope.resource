*** Settings ***
Documentation       Keywords for testing `tedge hsm` commands when `device.cryptoki.uri` restricts the
...                 tokens and keys tedge-p11-server can use.
...
...                 tedge-p11-server uses its `device.cryptoki.uri` as a base for every request URI, and
...                 when both set the same attribute, the server's value wins, so a request can be
...                 silently served from another token or key than requested. The commands detect this
...                 on the client side, which can't catch every case: tests asserting the correct
...                 behavior in those cases are tagged `known-issue:p11-server-uri-override` and skipped.

Resource            ../pkcs11_common.resource
Resource            ../create_key_hsm/create_key_hsm.resource


*** Keywords ***
Set up two tokens with a key each
    [Documentation]    Creates token1 and token2, with keys token1-key and token2-key respectively.
    ...    The config URI is unset while doing so, so the keys end up on the requested tokens.
    Execute Command    cmd=softhsm2-util --init-token --free --label token1 --pin=123456 --so-pin=123456
    Execute Command    cmd=softhsm2-util --init-token --free --label token2 --pin=123456 --so-pin=123456
    # restarts tedge-p11-server after creating the tokens, so it sees them
    Unset tedge-p11-server Uri
    Execute Command    cmd=tedge hsm create-key --type ecdsa --label token1-key "pkcs11:token=token1"
    Execute Command    cmd=tedge hsm create-key --type ecdsa --label token2-key "pkcs11:token=token2"
    Execute Command    cmd=tedge config unset device.key_uri

Set up an empty token
    [Arguments]    ${label}
    Execute Command
    ...    cmd=softhsm2-util --init-token --free --label ${label} --pin=123456 --so-pin=123456

Clean up tokens
    Unset tedge-p11-server Uri
    Execute Command    cmd=tedge config unset device.key_uri
    Execute Command    cmd=tedge config set device.cryptoki.pin 123456
    FOR    ${label}    IN    token1    token2    token3
        Execute Command    cmd=softhsm2-util --delete-token --token ${label} || true
    END

List all keys on token
    [Documentation]    Lists the keys on the token with the config URI unset, so all keys are listed.
    [Arguments]    ${token}    ${pin}=123456
    Unset tedge-p11-server Uri
    ${stdout}=    Execute Command    cmd=tedge hsm list-keys --pin ${pin} "pkcs11:token=${token}"
    RETURN    ${stdout}

List tokens ignoring the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2
    ${stdout}=    Execute Command    cmd=tedge hsm list-tokens
    Should Match Regexp    ${stdout}    Label:\\s+token1
    Should Match Regexp    ${stdout}    Label:\\s+token2

Initialize a token ignoring the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2
    ${stdout}=    Execute Command    cmd=tedge hsm init --label token3    strip=True
    Should Contain    ${stdout}    token=token3

List keys from a token excluded by the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm list-keys "pkcs11:token=token1"
    ...    stdout=False
    ...    stderr=True
    ...    strip=True
    Should Contain    ${stderr}    No keys were found on the token
    Should Contain    ${stderr}    2 results were not shown

List keys by slot id
    ${slot}=    Execute Command
    ...    cmd=tedge hsm list-tokens | grep -B1 -E 'Label: +token2$' | head -1 | awk '{print $2}'
    ...    strip=True
    ${stdout}=    Execute Command    cmd=tedge hsm list-keys "pkcs11:slot-id=${slot}"
    Should Contain    ${stdout}    Label: token2-key
    Should Not Contain    ${stdout}    Label: token1-key

Delete a key from a token excluded by the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2
    Command Should Fail With
    ...    tedge hsm delete-key --force --label token2-key "pkcs11:token=token1"
    ...    Refusing to delete keys
    ${keys}=    List all keys on token    token2
    Should Contain    ${keys}    Label: token2-key

Delete a key overridden by the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2;object=token2-key
    Command Should Fail With
    ...    tedge hsm delete-key --force --label other-key "pkcs11:token=token2"
    ...    Refusing to delete keys
    ${keys}=    List all keys on token    token2
    Should Contain    ${keys}    Label: token2-key

Report a key deleted because of the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2;object=token2-key
    ${stderr}=    Command Should Fail With
    ...    tedge hsm delete-key --force --label other-key "pkcs11:token=token2"
    ...    object(s) not matching
    Should Contain    ${stderr}    object=token2-key

Create a key on a token excluded by the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2
    Command Should Fail With
    ...    tedge hsm create-key --type ecdsa --label new-key "pkcs11:token=token1"
    ...    Refusing to create the key
    ${keys}=    List all keys on token    token2
    Should Not Contain    ${keys}    Label: new-key
    Execute Command    cmd=tedge config get device.key_uri    exp_exit_code=!0

Create a key when the configured URI selects an empty token
    Set up an empty token    token3
    Set tedge-p11-server Uri    pkcs11:token=token3
    Command Should Fail With
    ...    tedge hsm create-key --type ecdsa --label new-key "pkcs11:token=token1"
    ...    Refusing to create the key
    ${keys}=    List all keys on token    token3
    Should Not Contain    ${keys}    Label: new-key

Create a key whose label is overridden by the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2;object=token2-key
    ${stderr}=    Execute Command
    ...    cmd=tedge hsm create-key --type ecdsa --label new-key "pkcs11:token=token2"
    ...    stdout=False
    ...    stderr=True
    Should Contain    ${stderr}    New keypair was successfully created
    ${keys}=    List all keys on token    token2
    Should Contain    ${keys}    Label: new-key

Change the PIN of a token excluded by the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2
    Command Should Fail With
    ...    tedge hsm change-pin --current-pin 123456 --new-pin 654321 "pkcs11:token=token1"
    ...    Refusing to change the PIN
    List all keys on token    token2    pin=123456

Change the PIN when the configured URI selects an empty token
    Set up an empty token    token3
    Set tedge-p11-server Uri    pkcs11:token=token3
    Command Should Fail With
    ...    tedge hsm change-pin --current-pin 123456 --new-pin 654321 "pkcs11:token=token1"
    ...    Refusing to change the PIN
    List all keys on token    token3    pin=123456

Reset the PIN of a token excluded by the configured URI
    Set tedge-p11-server Uri    pkcs11:token=token2
    Command Should Fail With
    ...    tedge hsm change-pin --reset --so-pin 123456 --new-pin 654321 "pkcs11:token=token1"
    ...    Refusing to change the PIN
    List all keys on token    token2    pin=123456
