*** Settings ***
Documentation       `tedge hsm` commands when `device.cryptoki.uri` restricts tedge-p11-server.
...
...                 For create-key, delete-key and change-pin, tedge-p11-server only adds the attributes
...                 of the configured URI that are missing in the request, and fails with a "key not
...                 found" error if the two URIs set different values for the same attribute.
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
    List keys by slot id

delete-key refuses to delete a key from a token excluded by the configured URI
    Delete a key from a token excluded by the configured URI

delete-key refuses to delete a key other than requested because of the configured URI
    Delete a key overridden by the configured URI

delete-key uses the configured URI for attributes missing in the request
    Delete a key selected by the configured URI

create-key refuses to create a key on a token excluded by the configured URI
    Create a key on a token excluded by the configured URI

create-key refuses to create a key when the configured URI selects an empty token
    Create a key when the configured URI selects an empty token

create-key refuses to create a key when the configured URI selects another key
    Create a key whose label is overridden by the configured URI

change-pin refuses to change the PIN of a token excluded by the configured URI
    Change the PIN of a token excluded by the configured URI

change-pin refuses to change the PIN when the configured URI selects an empty token
    Change the PIN when the configured URI selects an empty token

change-pin --reset refuses to reset the PIN of a token excluded by the configured URI
    Reset the PIN of a token excluded by the configured URI
