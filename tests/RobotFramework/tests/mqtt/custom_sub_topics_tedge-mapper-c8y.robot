*** Settings ***
Documentation       Purpose of this test is to verify that tedge-mapper-c8y subscribes the topics that are configured as c8y.topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:c8y


*** Test Cases ***
Publish events to subscribed topic
    Execute Command    tedge mqtt pub tedge/events/event-type '{"text": "Event"}'
    Should Have MQTT Messages    c8y/s/us    message_pattern=400,event-type,"Event",*

Publish measurements to unsubscribed topic
    Execute Command    tedge mqtt pub tedge/measurements '{"temperature": 10}'
    Sleep    5s    reason=If a message is not published in 5s, it will never be published.
    Should Have MQTT Messages    c8y/measurement/measurements/create    minimum=0    maximum=0


*** Keywords ***
Custom Setup
    Setup
    Execute Command    sudo tedge config set c8y.topics tedge/events/+
    Execute Command    sudo systemctl restart tedge-mapper-c8y.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

Custom Teardown
    Get Logs
    Execute Command    sudo tedge config unset c8y.topics
