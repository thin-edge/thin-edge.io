*** Settings ***
Documentation       Purpose of this test is to verify that tedge-mapper-aws subscribes the topics that are configured as aws.topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:aws


*** Test Cases ***


Publish events to subscribed topic
    ${timestamp}=        Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///e/event-type '{"text": "someone logged-in"}'
    Should Have MQTT Messages    aws/td/device:main/e/event-type

Publish measurements to unsubscribed topic   
    Execute Command    tedge mqtt pub te/device/main///m/measurement-type '{"temperature": 30}'
    Sleep    5s    reason=If a message is not published in 5s, it will never be published.
    Should Have MQTT Messages    aws/td/device:main/m/measurement-type    minimum=0    maximum=0


*** Keywords ***
Custom Setup
    Setup
    Execute Command    sudo tedge config set aws.topics "te/+/+/+/+/e/+"
    Execute Command    sudo tedge config set mqtt.bridge.built_in false
    Execute Command    sudo systemctl start tedge-mapper-aws.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws

Custom Teardown
    Get Logs
    Execute Command    sudo tedge config unset aws.topics
