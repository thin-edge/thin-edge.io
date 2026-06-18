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

Bridge drops oversized cloud-bound message without blocking the connection
    [Documentation]    A cloud-bound message whose MQTT packet exceeds the broker's payload limit
    ...    would silently block the bridge connection if forwarded. The built-in bridge instead
    ...    drops it, logs a warning, and keeps forwarding subsequent messages.
    Cumulocity.Set Device    ${DEVICE_SN}

    # A within-limit inventory update is forwarded and reaches the cloud
    Execute Command
    ...    tedge mqtt pub "c8y/inventory/managedObjects/update/${DEVICE_SN}" '{"bridgeFilterTest":"before"}'
    Device Should Have Fragment Values    bridgeFilterTest\=before

    # Send an inventory update whose packet is well over the limit (~18 kB vs the 16184 B default)
    ${start_time}=    Get Unix Timestamp
    Execute Command
    ...    cmd=printf '{"oversized":"%s"}' "$(head -c 18000 /dev/zero | tr '\\0' 'x')" > /tmp/oversized.json
    Execute Command
    ...    cmd=tedge mqtt pub "c8y/inventory/managedObjects/update/${DEVICE_SN}" "$(cat /tmp/oversized.json)"

    # The cloud connection is still healthy after the oversized message is dropped
    Cloud Connection Should Be Healthy

    # A following within-limit update with a different payload still reaches the cloud,
    # proving the bridge kept working after dropping the oversized message
    Execute Command    tedge mqtt pub "c8y/inventory/managedObjects/update/${DEVICE_SN}" '{"bridgeFilterTest":"after"}'
    Device Should Have Fragment Values    bridgeFilterTest\=after

    # The bridge logged that it dropped the oversized message rather than forwarding it.
    # Checked last so the round-trips above guarantee the warning has reached the journal.
    Logs Should Contain
    ...    text=Dropping cloud-bound message on topic inventory/managedObjects/update/${DEVICE_SN}
    ...    date_from=${start_time}
    [Teardown]    Clear Oversized Message


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge reconnect c8y
    Set Managed Object    ${DEVICE_SN}

Clear Oversized Message
    # Even if the test fails part way, ensure nothing is left persisted in the local broker
    # that could be redelivered and affect later tests (the bridge uses a durable session).
    Execute Command    tedge mqtt pub --retain "c8y/inventory/managedObjects/update/${DEVICE_SN}" ''
    Execute Command    cmd=rm -f /tmp/oversized.json

Cloud Connection Should Be Healthy
    [Documentation]    Assert the Cumulocity bridge connection is up, with a failure message that
    ...    explains the likely cause (an oversized message forwarded to the cloud) so the cause is
    ...    clear from the test logs alone.
    ${output}=    Execute Command
    ...    cmd=tedge connect c8y --test 2>&1; echo "exit_code=$?"
    ...    timeout=10
    ...    ignore_exit_code=${True}
    Should Contain
    ...    ${output}
    ...    exit_code=0
    ...    msg=Cumulocity dropped the bridge connection: the bridge forwarded a cloud-bound MQTT message larger than Cumulocity's maximum packet size instead of filtering it, so the broker silently closed the connection and 'tedge connect c8y --test' exited non-zero. Output: ${output}
    ...    values=${False}
