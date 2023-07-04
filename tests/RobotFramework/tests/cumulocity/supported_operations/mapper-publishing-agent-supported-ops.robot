*** Settings ***
Resource    ../../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:operation    theme:tedge-agent
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Create and publish the tedge agent supported operations on mapper restart
    # stop mapper and remove the supported operations
    ThinEdgeIO.Stop Service     tedge-mapper-c8y
    Execute Command    sudo rm -rf /etc/tedge/operations/c8y/*
  
    # the operation files must not exist
    ThinEdgeIO.File Should Not Exist    /etc/tedge/operations/c8y/c8y_SoftwareUpdate
    ThinEdgeIO.File Should Not Exist    /etc/tedge/operations/c8y/c8y_Restart

    ${timestamp}=        Get Unix Timestamp
    # now restart the mapper
    ThinEdgeIO.start Service    tedge-mapper-c8y
    Should Have MQTT Messages    tedge/health/tedge-mapper-c8y     message_contains=up    date_from=${timestamp}
    # After receiving the health status `up` from tege-agent, the mapper creates supported operations and will publish to c8y
    Should Have MQTT Messages    tedge/health/tedge-agent     message_contains=up
    
    # Check if the `c8y_SoftwareUpdate` and `c8y_Restart` ops files exists in `/etc/tedge/operations/c8y` directory
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/c8y/c8y_SoftwareUpdate
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/c8y/c8y_Restart

    # Check if the tedge-agent supported operations exists in c8y cloud
    Cumulocity.Should Contain Supported Operations    c8y_Restart    c8y_SoftwareUpdate   


Agent gets the software list request once it comes up
    ${timestamp}=        Get Unix Timestamp    
    ThinEdgeIO.restart Service    tedge-agent
    # wait till there is up status on tedge-agent health
    Should Have MQTT Messages    tedge/health/tedge-agent    message_contains=up    date_from=${timestamp}
    # now there should be a new list request
    Should Have MQTT Messages    tedge/commands/req/software/list    message_contains=id    date_from=${timestamp}
   

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
   