*** Settings ***
Resource        pin_select.resource

Suite Setup     tedge-p11-server Setup    ${TEDGE_P11_SERVER_VERSION}


*** Variables ***
${TEDGE_P11_SERVER_VERSION}     ${EMPTY}


*** Test Cases ***
Can pass PIN in the request using pin-value
    Pass PIN in the request using pin-value

Can pass PIN in the request using device.key_pin
    Pass PIN in the request using device.key_pin
