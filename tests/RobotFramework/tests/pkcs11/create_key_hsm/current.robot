*** Settings ***
Documentation       Tests for the `tedge hsm create-key` command.

Resource            ../pkcs11_common.resource
Resource            ./create_key_hsm.resource

Suite Setup         Custom Setup    ${TEDGE_P11_SERVER_VERSION}
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     ${EMPTY}


*** Test Cases ***
Can create a private key on the PKCS11 token
    Create a private key on the PKCS11 token

Shows connected initialized tokens when token argument is not provided
    Show connected initialized tokens when token argument is not provided

Can set key ID using --id flag
    Set key ID using --id flag

Can provide PIN using --pin flag
    Provide PIN using --pin flag

Saves public key to file using --outfile-pubkey flag
    Save public key to file using --outfile-pubkey flag

Deprecated cert create-key-hsm alias still works
    Deprecated cert create-key-hsm alias still works

Can list keys filtered by label or id
    List keys filtered by label or id

create-key does not overwrite a key_uri that points to another existing key
    Do not overwrite a key_uri that points to another existing key

create-key updates a key_uri that points to a key that no longer exists
    Update key_uri when it points to a key that no longer exists
