*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 This suite focuses on testing selection and connecting to the cloud using different types of private
...                 keys stored in PKCS#11 tokens.
...
...                 Uses SoftHSM2 to simulate a hardware security module for testing purposes. In real production
...                 environments, a dedicated hardware device would be used.

# it would be good to explain here why we use the tedge-p11-server exclusively and not the module mode
Resource            ../pkcs11_common.resource
Resource            tedge_connect.resource

Suite Setup         Custom Setup    ${TEDGE_P11_SERVER_VERSION}
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Variables ***
${KEY_URI}                      ${EMPTY}
${TEDGE_P11_SERVER_VERSION}     1.6.0


*** Test Cases ***
Can use Private Key in SoftHSM2 using tedge-p11-server
    Use Private Key in SoftHSM2 using tedge-p11-server

Can select Private key using tedge-p11-server URI
    Select Private key using tedge-p11-server URI

Can select Private key using a request URI
    Select Private key using a request URI

Can connect to C8y using an RSA key
    Connects to C8y using an RSA key

Can connect to C8y supporting all TLS13 ECDSA signature algorithms
    Connects to C8y supporting all TLS13 ECDSA signature algorithms

# Can use PKCS11 key to renew the public certificate
#    Use PKCS11 key to renew the public certificate

# Can use tedge cert download c8y to download a certificate
#    Use tedge cert download c8y to download a certificate

# Can renew the certificate using different keypair
#    Renew the certificate using different keypair

# Can pass PIN in the request using pin-value
#    Pass PIN in the request using pin-value

# Can pass PIN in the request using device.key_pin
#    Pass PIN in the request using device.key_pin
