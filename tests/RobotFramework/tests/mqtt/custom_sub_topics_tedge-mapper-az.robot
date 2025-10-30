*** Settings ***
Documentation       Purpose of this test is to verify that tedge-mapper-az subscribes the topics that are configured as az.topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown
Test Timeout        5 minutes

Test Tags           theme:mqtt    theme:az


*** Test Cases ***
Publish events to subscribed topic
    Execute Command    tedge mqtt pub te/device/main///e/event-type '{"text": "Event"}'
    Should Have MQTT Messages    az/messages/events/#

Publish measurements to unsubscribed topic
    Execute Command    tedge mqtt pub te/device/main///m/ '{"temperature": 10}'
    Sleep    5s    reason=If a message is not published in 5s, it will never be published.
    Should Have MQTT Messages    az/messages/events/#    minimum=0    maximum=0    message_contains=temperature


*** Keywords ***
Custom Setup
    Setup
    Execute Command    sudo tedge config set az.topics te/+/+/+/+/e/+
    Execute Command    sudo tedge config set mqtt.bridge.built_in false
    Execute Command    sudo systemctl restart tedge-mapper-az.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-az

Custom Teardown
    Get Suite Logs
    Execute Command    sudo tedge config unset az.topics
