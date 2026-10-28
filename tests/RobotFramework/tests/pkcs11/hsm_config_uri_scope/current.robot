*** Settings ***
Documentation       `tedge hsm` commands when `device.cryptoki.uri` restricts tedge-p11-server.
...
...                 There is no `initial.robot` variant, as released tedge-p11-server versions can't
...                 list keys, delete keys, or change PINs.

Resource            ../pkcs11_common.resource
Resource            ./hsm_config_uri_scope.resource

Suite Setup         Custom Setup    ${TEDGE_P11_SERVER_VERSION}
Suite Teardown      Get Suite Logs
Test Setup          Set up two tokens with a key each
Test Teardown       Clean up tokens

Test Tags           adapter:docker    theme:cryptoki


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     ${EMPTY}


*** Test Cases ***
list-tokens lists tokens excluded by the configured URI
    List tokens ignoring the configured URI

init initializes a token excluded by the configured URI
    Initialize a token ignoring the configured URI

list-keys does not list keys from a token excluded by the configured URI
    List keys from a token excluded by the configured URI

list-keys lists keys of the token selected by slot id
    [Documentation]    Fails because tedge-p11-server ignores `slot-id` when selecting a token
    ...    (`Cryptoki::open_session`) and lists the keys of the first token instead.
    [Tags]    robot:skip    known-issue:p11-server-uri-override
    List keys by slot id

delete-key refuses to delete a key from a token excluded by the configured URI
    Delete a key from a token excluded by the configured URI

delete-key refuses to delete a key other than requested because of the configured URI
    [Documentation]    Fails because the `object` attribute of the configured URI overrides the
    ...    requested one (`Cryptoki::request_uri`), and listing keys ignores `object`/`id`
    ...    (`Cryptoki::list_keys`), so the client only finds out after the key was deleted.
    [Tags]    robot:skip    known-issue:p11-server-uri-override
    Delete a key overridden by the configured URI

delete-key reports a key deleted because of the configured URI
    Report a key deleted because of the configured URI

create-key refuses to create a key on a token excluded by the configured URI
    Create a key on a token excluded by the configured URI

create-key refuses to create a key when the configured URI selects an empty token
    [Documentation]    Fails because the configured URI overrides the requested token
    ...    (`Cryptoki::request_uri`) and the client can only identify the selected token by the
    ...    keys on it, so the key is created before the mismatch is detected.
    [Tags]    robot:skip    known-issue:p11-server-uri-override
    Create a key when the configured URI selects an empty token

create-key creates a new key when the configured URI selects another key
    [Documentation]    Fails because the `object` attribute of the configured URI overrides the
    ...    requested label in the existing key check (`Cryptoki::request_uri`), so the configured
    ...    key is reused instead.
    [Tags]    robot:skip    known-issue:p11-server-uri-override
    Create a key whose label is overridden by the configured URI

change-pin refuses to change the PIN of a token excluded by the configured URI
    Change the PIN of a token excluded by the configured URI

change-pin refuses to change the PIN when the configured URI selects an empty token
    [Documentation]    Fails because the configured URI overrides the requested token
    ...    (`Cryptoki::request_uri`) and the client can only identify the selected token by the
    ...    keys on it, so the PIN is changed before the mismatch is detected.
    [Tags]    robot:skip    known-issue:p11-server-uri-override
    Change the PIN when the configured URI selects an empty token

change-pin --reset refuses to reset the PIN of a token excluded by the configured URI
    [Documentation]    Fails because the configured URI overrides the requested token
    ...    (`Cryptoki::request_uri`), and the client skips its check for a reset, as listing keys
    ...    would need a login with the unknown current PIN.
    [Tags]    robot:skip    known-issue:p11-server-uri-override
    Reset the PIN of a token excluded by the configured URI
