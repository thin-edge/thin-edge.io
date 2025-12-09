*** Settings ***
Resource        tedge_cert_create.resource

Suite Setup     tedge-p11-server Setup    ${TEDGE_P11_SERVER_VERSION}


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     ${EMPTY}


*** Test Cases ***
Tedge cert create should use HSM configuration
    Test tedge cert create uses HSM configuration
