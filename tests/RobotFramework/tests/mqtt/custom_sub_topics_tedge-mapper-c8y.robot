*** Settings ***
Documentation       Purpose of this test is to verify that tedge-mapper-c8y subscribes the topics that are configured as c8y.topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:c8y


*** Test Cases ***
Publish events to subscribed topic
    Execute Command    tedge mqtt pub te/device/main///e/event-type '{"text": "Event"}'
    Should Have MQTT Messages    c8y/s/us    message_pattern=400,event-type,"Event",*

Publish measurements to unsubscribed topic
    Execute Command    tedge mqtt pub te/device/main///m/ '{"temperature": 10}'
    Should Not Have MQTT Messages    c8y/measurement/measurements/create


*** Keywords ***
Custom Setup
    Setup
    Execute Command    sudo tedge config set c8y.topics te/+/+/+/+/e/+
    Execute Command    sudo systemctl restart tedge-mapper-c8y.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

Custom Teardown
    Get Suite Logs
    Execute Command    sudo tedge config unset c8y.topics
