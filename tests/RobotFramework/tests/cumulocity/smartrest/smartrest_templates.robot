*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs


*** Test Cases ***

Add New SmartREST Template
    [Documentation]  Test to add a new SmartREST template

    # Check if SmartREST template setting exists
    Verify Non-Existing SmartREST Templates

    # Add Template and check if it was added
    Execute Command    sudo tedge config set c8y.smartrest.templates template-1
    Check SmartREST Templates    present    template-1

    # Add Template and check if existing template + added template co-exist
    Execute Command    sudo tedge config add c8y.smartrest.templates template-2    
    Reconnect to Cumulocity
    Check SmartREST Templates    present    template-1    template-2

    # Add Template using set command and check if setting was overwritten
    Execute Command    sudo tedge config set c8y.smartrest.templates template-2,template-3
    Reconnect to Cumulocity
    Check SmartREST Templates    present    template-2    template-3
    Check SmartREST Templates    not present    template-1

Remove SmartREST Template
    [Documentation]  Test to remove a SmartREST template
    Execute Command    sudo tedge config remove c8y.smartrest.templates template-2
    Reconnect to Cumulocity
    Check SmartREST Templates    present    template-3
    Check SmartREST Templates    not present    template-2

    # Remove all SmartREST templates using unset
    Execute Command    sudo tedge config unset c8y.smartrest.templates
    Reconnect to Cumulocity
    Verify Non-Existing SmartREST Templates


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN

Verify Non-Existing SmartREST Templates
    ${output}=    Execute Command    tedge config get c8y.smartrest.templates
    Should Match Regexp    ${output}    ^\s*(\\[\\]|\bnone\b|null)\s*$

Reconnect to Cumulocity
    Execute Command    sudo tedge reconnect c8y

Check SmartREST Templates
    [Arguments]    ${state}    @{templates}
    ${output}=    Execute Command    tedge config get c8y.smartrest.templates
    FOR    ${template}    IN    @{templates}
        IF    '${state}' == 'present'
            Should Contain    ${output}    ${template}
        ELSE IF    '${state}' == 'not present'
            Should Not Contain    ${output}    ${template}
        END
    END
