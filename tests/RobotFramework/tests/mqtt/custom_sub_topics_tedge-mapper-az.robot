*** Settings ***
Documentation       Purpose of this test is to verify that tedge-mapper-az subscribes the topics that are configured as az.topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:az


*** Test Cases ***
Publish measurements to subscribed topic
    Execute Command    tedge mqtt pub tedge/measurements/child '{"temperature": 10}'
    Should Have MQTT Messages    az/messages/events/#

Publish measurements to unsubscribed topic
    Execute Command    tedge mqtt pub tedge/measurements '{"temperature": 10}'
    Sleep    5s    reason=If a message is not published in 5s, it will never be published.
    Should Have MQTT Messages    az/messages/events/#    minimum=0    maximum=0


*** Keywords ***
Custom Setup
    Setup
    Execute Command    sudo tedge config set az.topics tedge/measurements/+
    Execute Command    sudo systemctl restart tedge-mapper-az.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-az

Custom Teardown
    Get Logs
    Execute Command    sudo tedge config unset az.topics
