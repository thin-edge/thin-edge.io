*** Settings ***
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_mapper

*** Test Cases ***
Add missing timestamps
    Execute Command    tedge mqtt pub te/device/main// '{}'
    ${transformed_msg}    Should Have MQTT Messages    gen-mapper/c8y
    Should Contain    ${transformed_msg}    item=time

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Copy Configuration Files
    Start Generic Mapper

Copy Configuration Files
    Execute Command    mkdir /etc/tedge/gen-mapper/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/pipelines/*    /etc/tedge/gen-mapper/

Start Generic Mapper
    Execute Command    nohup tedge run tedge-mapper gen &
