*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO    adapter=docker

Test Tags    theme:testing
Test Teardown    Get Logs

*** Test Cases ***

Supports a reconnect
    ${DEVICE_SN}                         Setup
    Device Should Exist                  ${DEVICE_SN}
    Disconnect Then Connect Mapper       mapper=c8y    sleep=5

Supports disconnect then connect
    ${DEVICE_SN}                         Setup
    Device Should Exist                  ${DEVICE_SN}
    Disconnect Mapper
    Connect Mapper

Update unknown setting
    ${DEVICE_SN}                         Setup
    Device Should Exist                  ${DEVICE_SN}
    Execute Command                      tedge config set unknown.value 1    exp_exit_code=2

Update known setting
    ${DEVICE_SN}                         Setup
    Set Tedge Configuration Using CLI    device.type        mycustomtype
    ${OUTPUT}=    Execute Command        tedge config get device.type
    Should Match    ${OUTPUT}            mycustomtype\n
