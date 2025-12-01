*** Settings ***
Documentation       Tests for the `tedge cert create-key-hsm` command.

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
