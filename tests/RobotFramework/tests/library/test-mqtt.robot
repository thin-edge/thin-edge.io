*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO    adapter=docker

Test Setup          Setup
Test Teardown       Get Logs

Test Tags           theme:testing


*** Test Cases ***
It checks MQTT messages using a pattern
    ${DEVICE_SN}    Setup
    Device Should Exist    ${DEVICE_SN}
    Execute Command    tedge mqtt pub 'custom/message' '{"status":"executing"}'
    Should Have MQTT Messages    custom/message    message_pattern=.*executing.*
    Should Have MQTT Messages    custom/message    message_contains=executing
