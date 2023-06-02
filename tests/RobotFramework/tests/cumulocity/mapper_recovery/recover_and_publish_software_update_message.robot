*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:mapper recovery 
Suite Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Mapper recovers and processes output of ongoing software update request
    [Documentation]    C8y-Mapper receives a software update request, 
    ...                Delegates operation to tedge-agent and gets back `executing` status
    ...                And then goes down (here purposefully stopped).
    ...                Mean while the agent processes the update message and publishes the software update message
    ...                After some time mapper recovers and pushes the result to c8y cloud
    ...                Verify that the rolldice package is installed or not
    ${timestamp}=        Get Unix Timestamp
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y    
    ${OPERATION}=    Install Software    rolldice,1.0.0::dummy
    Should Have MQTT Messages    tedge/commands/res/software/update    message_contains=executing    date_from=${timestamp}
    ThinEdgeIO.Stop Service    tedge-mapper-c8y    
    Should Have MQTT Messages    tedge/commands/res/software/update    message_contains=successful    date_from=${timestamp}
    ThinEdgeIO.Start Service    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y
    Operation Should Be SUCCESSFUL           ${OPERATION}    timeout=60
    Device Should Have Installed Software    rolldice
 
*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN 
    Device Should Exist                      ${DEVICE_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y
    # This acts as a custom sm plugin
    ThinEdgeIO.Transfer To Device     ${CURDIR}/custom_sw_plugin.sh    /etc/tedge/sm-plugins/dummy
