*** Settings ***
Documentation       Test thin-edge.io MQTT client authentication using a Hardware Security Module (HSM).
...
...                 To do this, we install SoftHSM2 which allows us to create software-backed PKCS#11 (cryptoki)
...                 cryptographic tokens that will be read by thin-edge. In real production environments a dedicated
...                 hardware device would be used.
...    
...                 In this test we use cryptoki module directly, so `tedge` binary has to be built with glibc:
...                 1. just release x86_64-unknown-linux-gnu
...                 2. invoke use-local --arch "x86_64*gnu" build --arch "x86_64*gnu"

Resource            ../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           adapter:docker    theme:cryptoki


*** Test Cases ***
Use cryptoki module directly
    # setup a software cryptographic token
    Execute Command    softhsm2-util --init-token --slot 0 --label "device-cert" --so-pin 123456 --pin 123456
    Execute Command    softhsm2-util \
    ...    --import <(cat "$(tedge config get device.key_path)" && cat "$(tedge config get device.cert_path)") \
    ...    --token "device-cert" \
    ...    --label device-cert \
    ...    --id 01 \
    ...    --pin 123456 \
    ...    --force
    
    # enable cryptoki to use the created token
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    Execute Command    tedge config set device.cryptoki.mode module

    Execute Command    tedge reconnect c8y

*** Keywords ***
Custom Setup
    Setup
    Execute Command    apt-get install -y gnutls-bin softhsm2

