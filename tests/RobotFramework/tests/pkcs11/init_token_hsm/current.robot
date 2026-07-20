*** Settings ***
Documentation       Tests for the `tedge hsm init` command and automatic token initialization.

Resource            ../pkcs11_common.resource
Resource            ./init_token_hsm.resource

Suite Setup         Custom Setup    ${TEDGE_P11_SERVER_VERSION}
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     ${EMPTY}


*** Test Cases ***
Auto-initializes a token when creating a key and none exists
    Auto initialize token when creating a key

create-key reuses an existing key by default
    create-key reuses an existing key by default

create-key --force-new creates a new key
    create-key --force-new creates a new key

Can initialize a token with hsm init
    Initialize a token with hsm init

hsm init is idempotent
    Initializing an existing token is idempotent
