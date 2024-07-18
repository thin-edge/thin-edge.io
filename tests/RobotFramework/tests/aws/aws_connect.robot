*** Settings ***
Documentation       Verify that thin-edge.io can successfully connect to AWS IoT Core
    ...             The test assumes that the Just in Time Provisioning (JITP) is setup in AWS

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Setup         Custom Setup
Test Teardown      Get Logs

Test Tags           theme:mqtt    theme:aws    test:on_demand


*** Test Cases ***

Connect to AWS using mosquitto bridge
    Execute Command    tedge config set mqtt.bridge.built_in false
    Execute Command    sudo tedge config set aws.url ${AWS_CONFIG.host}
    ${stdout}=    Execute Command    sudo tedge connect aws    retries=0
    Should Not Contain    ${stdout}    Warning: Bridge has been configured, but Aws connection check failed
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws
    Should Have MQTT Messages    te/device/main/service/mosquitto-aws-bridge/status/health    message_pattern=^1$


Connect to AWS using built-in bridge
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    sudo tedge config set aws.url ${AWS_CONFIG.host}
    ${stdout}=    Execute Command    sudo tedge connect aws    retries=0
    Should Not Contain    ${stdout}    Warning: Bridge has been configured, but Aws connection check failed
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws
    Should Have MQTT Messages    te/device/main/service/tedge-mapper-bridge-aws/status/health    message_contains="status":"up"


*** Keywords ***

Custom Setup
    Setup
