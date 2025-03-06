*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Logs

Test Tags           theme:cli    theme:mqtt


*** Test Cases ***
tedge mqtt sub breaks after 1s
    ${start_timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt sub "#" --duration 1s
    ${end_timestamp}=    Get Unix Timestamp
    Validate duration    ${start_timestamp}    ${end_timestamp}

tedge mqtt sub breaks after receiving 3 packets
    Execute Command    tedge mqtt pub -r test/topic/1 "one"
    Execute Command    tedge mqtt pub -r test/topic/2 "two"
    Execute Command    tedge mqtt pub -r test/topic/3 "three"
    ${output}=    Execute Command    tedge mqtt sub "test/topic/+" --count 3
    Should Contain    ${output}    one
    Should Contain    ${output}    two
    Should Contain    ${output}    three


*** Keywords ***
Validate duration
    [Arguments]    ${start_timestamp}    ${end_timestamp}
    IF    ${end_timestamp} - ${start_timestamp} > 5
        Fail
        ...    Must be less than 5s difference between end_timestamp (${end_timestamp}) and start_timestamp(${start_timestamp})
    END
