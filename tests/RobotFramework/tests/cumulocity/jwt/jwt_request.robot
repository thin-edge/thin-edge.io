*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO 

Test Tags    theme:c8y    theme:tokens
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Retrieve a JWT tokens
    ${start_time}=    Get Unix Timestamp    
    Execute Command    tedge mqtt pub c8y/s/uat ''    
    ${messages}=    Should Have MQTT Messages    c8y/s/dat    maximum=1    date_from=${start_time}
    Should Contain    ${messages[0]}    71

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Stop Service    tedge-mapper-c8y
    Stop Service    tedge-agent
    Stop Service    c8y-configuration-plugin
    Stop Service    c8y-log-plugin
    Should Have MQTT Messages    tedge/health/mosquitto-c8y-bridge
    Sleep    1s    wait just in case that the server responds to already sent messages 
