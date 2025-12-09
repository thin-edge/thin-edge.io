*** Settings ***
Documentation       This test suite runs the tests with tedge-p11-server pinned to a fixed version to ensure that new
...                 versions of thin-edge remain backwards compatible with tedge-p11-server's binary communication protocol. The
...                 scope of this test is limited to tedge-p11-server's initial feature set and will generally not be expanded.

Resource            ../pkcs11_common.resource
Resource            tedge_cert_renew.resource

Suite Setup         tedge-p11-server Setup    ${TEDGE_P11_SERVER_VERSION}
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki    compatibility


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     ${EMPTY}


*** Test Cases ***
Use Private Key in SoftHSM2 using tedge-p11-server
    Tedge Reconnect Should Succeed

Renew certificate
    Use PKCS11 key to renew the public certificate    error=PKCS #11 service failed: Failed to find a key

Can renew the certificate using different keypair
    Renew the certificate using different keypair
