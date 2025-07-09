*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs


*** Test Cases ***
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
