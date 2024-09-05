*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:tokens


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
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge reconnect c8y
    Should Have MQTT Messages    te/device/main/service/tedge-mapper-bridge-c8y/status/health
    Sleep    1s    wait just in case that the server responds to already sent messages
