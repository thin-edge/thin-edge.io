*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Suite Logs

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

tedge mqtt sub breaks after receiving first non-retailed message
    Execute Command    tedge mqtt pub -r foo/1 "bar1"
    Execute Command    tedge mqtt pub -r foo/2 "bar2"
    # clear any retained message
    Execute Command    tedge mqtt pub -r foo/non_retained ""

    # Start a subscription in the background (but it will still write to stdout), then send a
    # non-retained message which should stop the subscription early
    ${start_timestamp}=    Get Unix Timestamp
    ${output}=    Execute Command
    ...    tedge mqtt sub "foo/+" --duration 10s --retained-only & sleep 2 && tedge mqtt pub foo/non_retained "3" && wait
    ${end_timestamp}=    Get Unix Timestamp

    Should Be True
    ...    (${end_timestamp} - ${start_timestamp}) < 8
    ...    Duration should be less than the 10 second duration
    ${messages}=    Set Variable    ${output.splitlines()}
    Length Should Be    ${messages}    2
    Should Contain    ${messages[0]}    [foo/1] bar1
    Should Contain    ${messages[1]}    [foo/2] bar2


*** Keywords ***
Validate duration
    [Arguments]    ${start_timestamp}    ${end_timestamp}
    IF    ${end_timestamp} - ${start_timestamp} > 5
        Fail
        ...    Must be less than 5s difference between end_timestamp (${end_timestamp}) and start_timestamp(${start_timestamp})
    END
