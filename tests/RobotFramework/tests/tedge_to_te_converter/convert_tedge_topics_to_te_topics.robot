*** Settings ***
Documentation       Purpose of this test is to verify that tedge-agent converts the tedge/# topics to te/# topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:tedge to te


*** Test Cases ***
Convert main device measurement topic
    Execute Command    tedge mqtt pub tedge/measurements '{"temperature":25}'
    Should Have MQTT Messages    te/device/main///m/    message_pattern={"temperature":25}


Convert child device measurement topic
    Execute Command    tedge mqtt pub tedge/measurements/child '{"temperature":25}'
    Should Have MQTT Messages    te/device/child///m/    message_pattern={"temperature":25}

Convert main device event topic
    Execute Command    tedge mqtt pub tedge/events/login_event '{"text":"someone logedin"}'
    Should Have MQTT Messages    te/device/main///e/login_event    message_pattern={"text":"someone logedin"}

Convert child device event topic
    Execute Command    tedge mqtt pub tedge/events/login_event/child '{"text":"someone logedin"}'
    Should Have MQTT Messages    te/device/child///e/login_event    message_pattern={"text":"someone logedin"}

Convert main device alarm topic
    Execute Command    tedge mqtt pub tedge/alarms/minor/test_alarm '{"text":"test alarm"}' -q 2 -r
    Should Have MQTT Messages    te/device/main///a/test_alarm    message_contains=minor

Convert child device alarm topic
    Execute Command    tedge mqtt pub tedge/alarms/major/test_alarm/child '{"text":"test alarm"}' -q 2 -r
    Should Have MQTT Messages    te/device/child///a/test_alarm    message_contains=major


*** Keywords ***
Custom Setup
    Setup
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Custom Teardown
    Get Logs
