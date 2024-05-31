*** Settings ***
Documentation       Purpose of this test is to verify telemetry topics of tedge-mapper-aws

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:aws


*** Test Cases ***
Publish measurements to te measurement topic
    ${timestamp}=        Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///m/ '{"temperature": 10}'
    Should Have MQTT Messages    aws/td/device:main/m/    message_contains="temperature"    date_from=${timestamp}   minimum=1    maximum=1

Publish measurements to te measurement topic with measurement type
    ${timestamp}=        Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///m/test-type '{"temperature": 10}'
    Should Have MQTT Messages    aws/td/device:main/m/test-type   message_contains="temperature"    date_from=${timestamp}   minimum=1    maximum=1

Publish service measurements to te measurement topic with measurement type
    ${timestamp}=        Get Unix Timestamp  
    Execute Command    tedge mqtt pub te/device/main/service/test_service/m/test-type '{"temperature": 10}'
    Should Have MQTT Messages    aws/td/device:main:service:test_service/m/test-type   message_contains="temperature"    date_from=${timestamp}   minimum=1    maximum=1

Publish child measurements to te measurement topic with measurement type
    ${timestamp}=        Get Unix Timestamp   
    Execute Command    tedge mqtt pub te/device/child///m/test-type '{"temperature": 10}'
    Should Have MQTT Messages    aws/td/device:child/m/test-type   message_contains="temperature"    date_from=${timestamp}   minimum=1    maximum=1


Publish main device event to te event topic with event type
    ${timestamp}=        Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///e/event-type '{"text": "someone logged-in"}'
    Should Have MQTT Messages    aws/td/device:main/e/event-type   message_contains="someone logged-in"    date_from=${timestamp}   minimum=1    maximum=1     


Publish child device event to te event topic with event type
    ${timestamp}=        Get Unix Timestamp  
    Execute Command    tedge mqtt pub te/device/child///e/event-type '{"text": "someone logged-in"}'
    Should Have MQTT Messages    aws/td/device:child/e/event-type   message_contains="someone logged-in"    date_from=${timestamp}   minimum=1    maximum=1


Publish main device alarm to te alarm topic with alarm type
    ${timestamp}=        Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main///a/alarm-type '{"severity":"critical","text": "someone logged-in"}'
    Should Have MQTT Messages    aws/td/device:main/a/alarm-type   message_contains="critical"    date_from=${timestamp}   minimum=1    maximum=1     


Publish child device alarm to te alarm topic with alarm type
    ${timestamp}=        Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/child///a/alarm-type '{"severity":"major","text": "someone logged-in"}'
    Should Have MQTT Messages    aws/td/device:child/a/alarm-type   message_contains="major"    date_from=${timestamp}   minimum=1    maximum=1

Publish health status message for main device service
    Execute Command    tedge mqtt pub te/device/main/service/test-service/status/health '{"status":"up"}'
    Should Have MQTT Messages    aws/td/device:main:service:test-service/status/health    message_contains="status":"up"

*** Keywords ***
Custom Setup
    Setup
    Execute Command    tedge config set mqtt.bridge.built_in false
    Execute Command    sudo systemctl start tedge-mapper-aws.service
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-aws

Custom Teardown
    Get Logs
    Execute Command    sudo tedge config unset aws.topics
