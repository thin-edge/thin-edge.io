*** Settings ***
Resource        tedge_cert_download.resource

Suite Setup     tedge-p11-server Setup    ${TEDGE_P11_SERVER_VERSION}


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     1.7.0
${PKCS11_USE_P11TOOL}           ${True}


*** Test Cases ***
Can use tedge cert download c8y to download a certificate
    Use tedge cert download c8y to download a certificate
