*** Settings ***
Documentation       Purpose of this test is to verify that tedge-mapper-aws subscribes the topics that are configured as aws.topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:aws


*** Test Cases ***
Publish events to subscribed topic
    Execute Command    tedge mqtt pub tedge/events/event-type '{"text": "Event"}'
    Should Have MQTT Messages    aws/td/events/event-type

Publish measurements to unsubscribed topic
    Execute Command    tedge mqtt pub tedge/measurements '{"temperature": 10}'
    Sleep    5s    reason=If a message is not published in 5s, it will never be published.
    Should Have MQTT Messages    aws/td/measurements    minimum=0    maximum=0


*** Keywords ***
Custom Setup
    Setup
    Execute Command    sudo tedge config set aws.topics "tedge/events/+"
    Execute Command    sudo systemctl start tedge-mapper-aws.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws

Custom Teardown
    Get Logs
    Execute Command    sudo tedge config unset aws.topics
