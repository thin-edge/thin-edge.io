*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO
Library    JSONLibrary

Test Tags    theme:c8y    theme:firmware
Suite Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Firmware plugin should resend the firmware update request after being restarted
    ${binary_url}=    Cumulocity.Create Inventory Binary    firmware    binary    contents=content1
    Cumulocity.Set Device    ${CHILD_SN}
    ${operation}=    Cumulocity.Install Firmware    firmware    1.0.0    ${binary_url}

    # Wait for first message to be sent
    ${operation_start}=    ThinEdgeIO.Get Unix Timestamp
    ThinEdgeIO.Set Device Context    ${DEVICE_SN}
    ${messages}=    ThinEdgeIO.Should Have MQTT Messages    topic=tedge/${CHILD_SN}/commands/req/firmware_update    date_from=${operation_start}    minimum=1    maximum=1
    ${message}=    JSONLibrary.Convert String To Json    ${messages[0]}
    Should Be Equal    ${message["attempt"]}    ${1}
    
    # Restart firmware plugin
    ${restart_pre}=    ThinEdgeIO.Get Unix Timestamp
    Restart Service    c8y-firmware-plugin
    ${restart_post}=    ThinEdgeIO.Get Unix Timestamp
    Should Be True    ${restart_post} - ${restart_pre} < 30    msg=Service should not timeout when trying to restart #1932

    # Same request should be sent (but with the attempt counter increased by 1)
    ${messages}=    ThinEdgeIO.Should Have MQTT Messages    topic=tedge/${CHILD_SN}/commands/req/firmware_update    date_from=${restart_pre}    minimum=1    maximum=1
    ${message}=    JSONLibrary.Convert String To Json    ${messages[0]}

    Should Be Equal    ${message["attempt"]}    ${2}

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}_child1
    Execute Command    mkdir -p /etc/tedge/operations/c8y/${CHILD_SN}
    Restart Service    tedge-mapper-c8y
    Device Should Exist                      ${DEVICE_SN}
    Device Should Exist                      ${CHILD_SN}

    Service Health Status Should Be Up    tedge-mapper-c8y
