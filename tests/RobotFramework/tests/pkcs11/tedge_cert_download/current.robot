*** Settings ***
Resource        tedge_cert_download.resource

Suite Setup     tedge-p11-server Setup    ${TEDGE_P11_SERVER_VERSION}


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     ${EMPTY}


*** Test Cases ***
Can use tedge cert download c8y to download a certificate
    Use tedge cert download c8y to download a certificate

cert download does not create a key when key_uri selects none of the existing keys
    Fail cert download when key_uri does not select any of the existing keys

cert download creates a key automatically when the token holds none
    Automatically create a key when downloading a certificate and the token holds none
