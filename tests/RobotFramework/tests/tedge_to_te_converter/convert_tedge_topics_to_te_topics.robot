*** Settings ***
Documentation       Purpose of this test is to verify that tedge-agent converts the tedge/# topics to te/# topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             JSONLibrary

Test Setup         Custom Setup
Test Teardown      Get Logs

Test Tags           theme:mqtt    theme:tedge_to_te


*** Test Cases ***
Convert main device measurement topic
    Execute Command    tedge mqtt pub tedge/measurements ''
    ${messages_tedge}=    Should Have MQTT Messages    tedge/measurements    minimum=1    maximum=1
    ${messages_te}=    Should Have MQTT Messages    te/device/main///m/    minimum=1    maximum=1
    Should Be Equal    ${messages_tedge}    ${messages_te}

Convert main device empty measurement topic
    Execute Command    tedge mqtt pub tedge/measurements '{"temperature":20}'
    ${messages_tedge}=    Should Have MQTT Messages
    ...    tedge/measurements
    ...    minimum=1
    ...    maximum=1
    ...    message_contains=20
    ${messages_te}=    Should Have MQTT Messages
    ...    te/device/main///m/
    ...    minimum=1
    ...    maximum=1
    ...    message_contains=20
    Should Be Equal    ${messages_tedge}    ${messages_te}
    Should Have MQTT Messages    te/device/main///m/    message_pattern={"temperature":20}

Convert child device measurement topic
    Execute Command    tedge mqtt pub tedge/measurements/child '{"temperature":25}'
    ${messages_tedge}=    Should Have MQTT Messages
    ...    tedge/measurements/child
    ...    minimum=1
    ...    maximum=1
    ...    message_contains=25
    ${messages_te}=    Should Have MQTT Messages
    ...    te/device/child///m/
    ...    minimum=1
    ...    maximum=1
    ...    message_contains=25
    Should Be Equal    ${messages_tedge}    ${messages_te}
    Should Have MQTT Messages    te/device/child///m/    message_pattern={"temperature":25}

Convert main device event topic
    Execute Command    tedge mqtt pub tedge/events/login_event '{"text":"someone loggedin"}'
    ${messages_tedge}=    Should Have MQTT Messages
    ...    tedge/events/login_event
    ...    minimum=1
    ...    maximum=1
    ...    message_contains=someone loggedin
    ${messages_te}=    Should Have MQTT Messages
    ...    te/device/main///e/login_event
    ...    minimum=1
    ...    maximum=1
    ...    message_contains=someone loggedin
    Should Be Equal    ${messages_tedge}    ${messages_te}
    Should Have MQTT Messages    te/device/main///e/login_event    message_pattern={"text":"someone loggedin"}

Convert main device empty event topic
    Execute Command    tedge mqtt pub tedge/events/login_event2 ''
    ${messages_tedge}=    Should Have MQTT Messages    tedge/events/login_event2    minimum=1    maximum=1
    ${messages_te}=    Should Have MQTT Messages    te/device/main///e/login_event2    minimum=1    maximum=1
    Should Be Equal    ${messages_tedge}    ${messages_te}

Convert child device event topic
    Execute Command    tedge mqtt pub tedge/events/login_event/child '{"text":"someone loggedin 2"}'
    ${messages_tedge}=    Should Have MQTT Messages
    ...    tedge/events/login_event/child
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="someone loggedin 2"
    ${messages_te}=    Should Have MQTT Messages
    ...    te/device/child///e/login_event
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="someone loggedin 2"
    Should Be Equal    ${messages_tedge}    ${messages_te}
    Should Have MQTT Messages    te/device/child///e/login_event    message_pattern={"text":"someone loggedin 2"}

Convert main device alarm topic
    Execute Command    tedge mqtt pub tedge/alarms/minor/test_alarm '{"text":"test alarm 1"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///a/test_alarm
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="test alarm 1"
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal    ${message["severity"]}    minor

Convert main device alarm topic and retain
    Execute Command    tedge mqtt pub tedge/alarms/minor/test_alarm '{"text":"test alarm 2"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///a/test_alarm
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="test alarm 2"
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal    ${message["severity"]}    minor
    # Check if the retained message received with new client or not
    ${result}=    Execute Command    tedge mqtt sub --duration 2s --count 1 te/device/main///a/test_alarm
    Should Contain    ${result}    "severity":"minor"

Convert child device alarm topic
    Execute Command    tedge mqtt pub tedge/alarms/major/test_alarm/child1 '{"text":"test alarm 3"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages
    ...    te/device/child1///a/test_alarm
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="test alarm 3"
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal    ${message["severity"]}    major

Convert clear alarm topic
    Execute Command    tedge mqtt pub tedge/alarms/major/test_alarm/child2 '' -q 2 -r
    ${messages_tedge}=    Should Have MQTT Messages    tedge/alarms/major/test_alarm/child2    minimum=1    maximum=1
    ${messages_te}=    Should Have MQTT Messages    te/device/child2///a/test_alarm    minimum=1    maximum=1
    Should Be Equal    ${messages_tedge}    ${messages_te}

Convert empty alarm message
    Execute Command    tedge mqtt pub tedge/alarms/major/test_alarm/child3 {} -q 2 -r
    ${messages}=    Should Have MQTT Messages    te/device/child3///a/test_alarm    minimum=1    maximum=1
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal    ${message["severity"]}    major


*** Keywords ***
Custom Setup
    Setup
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
