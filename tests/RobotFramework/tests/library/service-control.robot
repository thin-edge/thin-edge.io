*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:testing

*** Test Cases ***

Support starting and stopping services
    ${DEVICE_SN}                     Setup
    Device Should Exist              ${DEVICE_SN}
    Process Should Be Running        tedge[_-]mapper c8y
    Stop Service                     tedge-mapper-c8y
    Process Should Not Be Running    tedge[_-]mapper c8y
    Start Service                    tedge-mapper-c8y
    Process Should Be Running        tedge[_-]mapper c8y
