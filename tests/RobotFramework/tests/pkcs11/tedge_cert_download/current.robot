*** Settings ***
Resource        tedge_cert_download.resource

Suite Setup     tedge-p11-server Setup    ${TEDGE_P11_SERVER_VERSION}


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     ${EMPTY}


*** Test Cases ***
Can use tedge cert download c8y to download a certificate
    Use tedge cert download c8y to download a certificate
