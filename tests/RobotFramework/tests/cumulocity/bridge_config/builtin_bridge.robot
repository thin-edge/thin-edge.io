*** Settings ***
Resource            ../../../resources/common.resource
Library             OperatingSystem
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs


*** Test Cases ***
Bridge inspect shows expected topic mappings
    [Documentation]    Verify that tedge bridge inspect c8y outputs the expected topic mappings
    ${output}=    Execute Command    tedge bridge inspect c8y    strip=${True}
    # Remove the "Reading from:" line as the path varies per environment
    ${output}=    Remove String Using Regexp    ${output}    Reading from:.*\n
    ${expected}=    OperatingSystem.Get File    ${CURDIR}/bridge_inspect_c8y.expected
    Should Be Equal    ${output.strip()}    ${expected.strip()}

Bridge test shows matching outbound rule
    [Documentation]    Verify that tedge bridge test c8y shows the correct routing for a local topic
    ${output}=    Execute Command    tedge bridge test c8y c8y/s/us/123    strip=${True}
    Should Contain    ${output}    [local] c8y/s/us/123    ->    [remote] s/us/123 (outbound)

Bridge test shows matching inbound rule
    [Documentation]    Verify that tedge bridge test c8y shows the correct routing for a remote topic
    ${output}=    Execute Command    tedge bridge test c8y s/dat    strip=${True}
    Should Contain    ${output}    [remote] s/dat    ->    [local] c8y/s/dat (inbound)

Bridge test rejects wildcard topics
    [Documentation]    Verify that tedge bridge test c8y rejects wildcard topics with exit code 1
    ${output}=    Execute Command
    ...    tedge bridge test c8y 'c8y/s/us/#'
    ...    exp_exit_code=1
    ...    stderr=${True}
    ...    timeout=0
    ...    retries=0
    Should Contain    ${output}[1]    Wildcard characters (#, +) are not supported

Bridge test exits with code 2 for non-matching topics
    [Documentation]    Verify that tedge bridge test c8y exits with code 2 when no rules match
    ${output}=    Execute Command
    ...    tedge bridge test c8y nonexistent/topic
    ...    exp_exit_code=2
    ...    strip=${True}
    ...    timeout=0
    ...    retries=0
    Should Contain    ${output}    No matching bridge rule found for "nonexistent/topic"

Connection test
    [Documentation]    Repeatedly test the cloud connection
    FOR    ${attempt}    IN RANGE    0    10    1
        ${output}=    Execute Command    tedge connect c8y --test    timeout=10
        Should Not Contain    ${output}    connection check failed
    END

Support publishing QoS 0 messages to c8y topic #2960
    [Documentation]    Verify the publishing of multiple QoS 0 message directly to the cloud connection
    ...    Note 1: Since QoS 0 aren't guaranteed to be delivered, use a non-strict assertion on the exact event count in the cloud.
    ...    During testing the test would reliably fail if the expected count is less than 10 messages.
    ...    Note 2: The bridge will automatically change the QoS from 0 when translating messages from te/# to c8y/#,
    ...    we can't use the te/# topics in the test.
    FOR    ${attempt}    IN RANGE    0    20    1
        Execute Command
        ...    tedge mqtt pub -q 0 c8y/s/us '400,test_q0,Test event with qos 0 attempt ${attempt}'
        ...    timeout=10
    END
    Cumulocity.Device Should Have Event/s    expected_text=Test event with qos 0.*    type=test_q0    minimum=10


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge reconnect c8y
    Set Managed Object    ${DEVICE_SN}
