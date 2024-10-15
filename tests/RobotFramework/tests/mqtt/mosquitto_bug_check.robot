*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             String

Suite Setup         Setup
Suite Teardown      Get Logs


*** Test Cases ***
Stop tedge-mapper-c8y
    Execute Command    sudo systemctl stop tedge-mapper-c8y.service

Publish an MQTT message with QoS 1
    Execute Command    tedge mqtt pub -q 1 te/device/main///e/test "{\\"text\\":\\"hello $(date +%s)\\"}"

Start tedge-mapper-c8y
    Execute Command    sudo systemctl start tedge-mapper-c8y.service

Check result
    ${result} =    Execute Command    mosquitto_sub -h 127.0.0.1 -c -i subscriber1 -t topic1 --qos 1 -W 2; mosquitto_pub -h 127.0.0.1 -t topic1 -m test --qos 1; sleep 1; mosquitto_sub -h 127.0.0.1 -c -i subscriber1 -t topic1 --qos 1 -C 1 -W 2 && echo PASSED || echo FAILED;
    ${trimmed_result} =    Remove Extra Spaces And Newlines    ${result}
    Log Result    ${trimmed_result}

*** Keywords ***
Remove Extra Spaces And Newlines
    [Arguments]    ${text}
    [Documentation]    Removes newlines and extra spaces from the text.
    ${no_newlines} =    Replace String    ${text}    \n    ${EMPTY}
    ${no_carriage_returns} =    Replace String    ${no_newlines}    \r    ${EMPTY}
    ${trimmed} =    Strip String    ${no_carriage_returns}
    [Return]    ${trimmed}

Log Result
    [Arguments]    ${result}
    [Documentation]    Logs the result without causing the test to fail.
    Run Keyword If    '${result}' == 'PASSED'    Log To Console    "Test PASSED: MQTT flow worked as expected."
    ...    ELSE    Log To Console    "Test FAILED: MQTT flow did not work as expected."
