*** Settings ***
Documentation       Verify that thin-edge.io can successfully connect to AWS IoT Core
...                 The test assumes that the Just in Time Provisioning (JITP) is setup in AWS

Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             AWS

Test Setup          Custom Setup
Test Teardown       Get Logs
Test Template       Connect to AWS

Test Tags           theme:mqtt    theme:aws    test:on_demand


*** Test Cases ***
Connect to AWS using mosquitto bridge    builtin_bridge=false
Connect to AWS using built-in bridge    builtin_bridge=true


*** Keywords ***
Custom Setup
    Setup

Connect to AWS
    [Arguments]    ${builtin_bridge}
    Execute Command    tedge config set mqtt.bridge.built_in ${builtin_bridge}
    ${aws_url}=    AWS.Get IoT URL
    Execute Command    sudo tedge config set aws.url ${aws_url}
    ${stdout}=    Execute Command    sudo tedge connect aws    retries=0
    Should Not Contain    ${stdout}    Warning: Bridge has been configured, but Aws connection check failed

    ${bridge_service_name}=    Get Bridge Service Name    aws
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws
    ThinEdgeIO.Should Have MQTT Messages
    ...    te/device/main/service/${bridge_service_name}/status/health
    ...    message_pattern=^(1|.*"status":"up".*)$
