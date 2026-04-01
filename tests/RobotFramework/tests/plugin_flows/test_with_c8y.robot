*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Custom Suite Setup
Test Teardown       Custom Test Teardown

Test Tags           theme:plugins    theme:flows    theme:c8y


*** Test Cases ***
List flows
    Transfer To Device    ${CURDIR}/default-flow    /etc/tedge/mappers/local/flows/
    # restart tedge-agent to send a new software list
    Restart Service    tedge-agent

    ${tedge_version}=    Execute Command    cmd=tedge --version | cut -d' ' -f2    strip=${True}
    ${expected_version}=    ThinEdgeIO.Escape Pattern    ${tedge_version}    is_json=${True}

    Device Should Have Installed Software
    ...    {"softwareType": "flow", "name": "c8y/alarms", "version": "${expected_version}"}
    ...    {"softwareType": "flow", "name": "c8y/events", "version": "${expected_version}"}
    ...    {"softwareType": "flow", "name": "c8y/measurements", "version": "${expected_version}"}
    ...    {"softwareType": "flow", "name": "c8y/health", "version": "${expected_version}"}
    ...    {"softwareType": "flow", "name": "c8y/units", "version": "${expected_version}"}
    ...    {"softwareType": "flow", "name": "local/default-flow", "version": "1.0.0"}
    ...    {"softwareType": "flow", "name": "local/default-flow/other", "version": "3.0.0"}
    ...    {"softwareType": "flow", "name": "local/default-flow/layer1", "version": "0.0.0"}
    ...    {"softwareType": "flow", "name": "local/default-flow/layer1/layer2", "version": "0.0.0"}
    ...    {"softwareType": "flow", "name": "local/default-flow/layer1/layer2/other", "version": "0.0.0"}

Install a new flow
    ${OPERATION}=    Install Software
    ...    {"name": "local/hello-flow", "version": "1.0.0", "softwareType": "flow", "url": "${V1_URL}"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software    {"softwareType": "flow", "name": "local/hello-flow", "version": "1.0.0"}
    Verify a flow is working    message_contains=hello to Bob

Install a new nested flow
    ${OPERATION}=    Install Software
    ...    {"name": "local/nested/hello-flow", "version": "1.0.0", "softwareType": "flow", "url": "${V1_URL}"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software
    ...    {"softwareType": "flow", "name": "local/nested/hello-flow", "version": "1.0.0"}
    Verify a flow is working    message_contains=hello to Bob

Install fails with invalid module name
    ${OPERATION}=    Install Software
    ...    {"name": "local/../bad-flow", "version": "1.0.0", "softwareType": "flow", "url": "${V1_URL}"}
    Operation Should Be FAILED    ${OPERATION}    timeout=60

Install fails with an invalid flow
    ${OPERATION}=    Install Software
    ...    {"name": "local/invalid-flow", "version": "1.0.0", "softwareType": "flow", "url": "${INVALID_FLOW_URL}"}
    Operation Should Be FAILED    ${OPERATION}    timeout=60
    Device Should Not Have Installed Software    {"softwareType": "flow", "name": "local/invalid-flow"}

Update a flow without params.toml
    [Setup]    Transfer hello-flow v1.0.0    with_params_toml=False
    ${OPERATION}=    Install Software
    ...    {"name": "local/hello-flow", "version": "2.0.0", "softwareType": "flow", "url": "${V2_URL}"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software    {"softwareType": "flow", "name": "local/hello-flow", "version": "2.0.0"}
    Verify a flow is working    message_contains=hi to Bob

Update a flow with params.toml
    [Setup]    Transfer hello-flow v1.0.0    with_params_toml=True
    ${OPERATION}=    Install Software
    ...    {"name": "local/hello-flow", "version": "2.0.0", "softwareType": "flow", "url": "${V2_URL}"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software    {"softwareType": "flow", "name": "local/hello-flow", "version": "2.0.0"}
    Verify a flow is working    message_contains=hi to Mary

Uninstall a flow entirely
    [Setup]    Transfer hello-flow v1.0.0    with_params_toml=False
    ${OPERATION}=    Uninstall Software
    ...    {"name": "local/hello-flow", "softwareType": "flow"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Not Have Installed Software    {"softwareType": "flow", "name": "local/hello-flow"}
    Directory Should Not Exist    /etc/tedge/mappers/local/flows/hello-flow
    Verify a flow is not working

Uninstall a flow with keeping params.toml
    [Setup]    Transfer hello-flow v1.0.0    with_params_toml=True
    Execute Command    sudo tedge config set flows.params.keep_on_delete true
    ${OPERATION}=    Uninstall Software
    ...    {"name": "local/hello-flow", "softwareType": "flow"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Not Have Installed Software    {"softwareType": "flow", "name": "local/hello-flow"}
    Directory Should Exist    /etc/tedge/mappers/local/flows/hello-flow
    File Should Exist    /etc/tedge/mappers/local/flows/hello-flow/params.toml
    File Should Not Exist    /etc/tedge/mappers/local/flows/hello-flow/flow.toml
    File Should Not Exist    /etc/tedge/mappers/local/flows/hello-flow/main.js
    File Should Not Exist    /etc/tedge/mappers/local/flows/hello-flow/params.toml.template
    Verify a flow is not working

Remove only .toml file
    [Setup]    Transfer To Device    ${CURDIR}/default-flow    /etc/tedge/mappers/local/flows/
    ${OPERATION}=    Uninstall Software
    ...    {"name": "local/default-flow/layer1/other", "softwareType": "flow"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Not Have Installed Software    {"softwareType": "flow", "name": "local/default-flow/layer1/other"}
    File Should Not Exist    /etc/tedge/mappers/local/flows/default-flow/layer1/other.toml


*** Keywords ***
Custom Suite Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}
    Transfer To Device    ${CURDIR}/hello-flow_1.0.0    /usr/local/tedge/hello-flow
    ${V1_URL}=    Cumulocity.Create Inventory Binary
    ...    local/hello-flow_1.0.0
    ...    package
    ...    file=${CURDIR}/hello-flow_1.0.0.tar.gz
    Set Suite Variable    ${V1_URL}

    ${V2_URL}=    Cumulocity.Create Inventory Binary
    ...    local/hello-flow_2.0.0
    ...    package
    ...    file=${CURDIR}/hello-flow_2.0.0.tar.gz
    Set Suite Variable    ${V2_URL}

    ${INVALID_FLOW_URL}=    Cumulocity.Create Inventory Binary
    ...    local/invalid-flow
    ...    package
    ...    file=${CURDIR}/invalid-flow.tar.gz
    Set Suite Variable    ${INVALID_FLOW_URL}

    Start Service    tedge-mapper-local

Custom Test Teardown
    Execute Command    cmd=rm -rf /etc/tedge/mappers/local/flows/*
    Get Logs

Transfer hello-flow v1.0.0
    [Arguments]    ${with_params_toml}=${False}
    Execute Command    cp -r /usr/local/tedge/hello-flow /etc/tedge/mappers/local/flows/hello-flow
    IF    '${with_params_toml}'=='True'
        Execute Command
        ...    cmd=cp /etc/tedge/mappers/local/flows/hello-flow/params.toml.template /etc/tedge/mappers/local/flows/hello-flow/params.toml
        Execute Command
        ...    cmd=sed -i 's/call = "Bob"/call = "Mary"/' /etc/tedge/mappers/local/flows/hello-flow/params.toml
    END

Verify a flow is working
    [Arguments]    ${message_contains}
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub hello/in ''
    Should Have MQTT Messages
    ...    topic=hello/out
    ...    message_contains=${message_contains}
    ...    date_from=${timestamp}

Verify a flow is not working
    ${timestamp}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub hello/in ''
    Should Not Have MQTT Messages
    ...    topic=hello/out
    ...    date_from=${timestamp}
