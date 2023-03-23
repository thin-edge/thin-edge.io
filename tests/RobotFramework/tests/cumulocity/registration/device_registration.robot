*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:registration
Suite Setup    Custom Setup
Test Teardown    Get Logs    ${DEVICE_SN}

*** Test Cases ***

Main device registration
    ${mo}=    Device Should Exist              ${DEVICE_SN}
    ${mo}=    Cumulocity.Device Should Have Fragment Values    name\=${DEVICE_SN}
    Should Be Equal    ${mo["owner"]}    device_${DEVICE_SN}
    Should Be Equal    ${mo["name"]}    ${DEVICE_SN}


Child device registration
    ThinEdgeIO.Set Device Context    ${DEVICE_SN}
    Execute Command    mkdir -p /etc/tedge/operations/c8y/${CHILD_SN}
    Restart Service    tedge-mapper-c8y

    # Check registration
    ${child_mo}=    Device Should Exist        ${CHILD_SN}
    ${child_mo}=    Cumulocity.Device Should Have Fragment Values    name\=${CHILD_SN}
    Should Be Equal    ${child_mo["owner"]}    device_${DEVICE_SN}    # The parent is the owner of the child
    Should Be Equal    ${child_mo["name"]}     ${CHILD_SN}

    # Check child device relationship
    Cumulocity.Set Device    ${DEVICE_SN}
    Cumulocity.Device Should Have A Child Devices    ${CHILD_SN}

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=                    Setup
    Set Suite Variable               $DEVICE_SN

    ${CHILD_SN}=                     Setup    bootstrap=${False}
    Set Suite Variable               $CHILD_SN
