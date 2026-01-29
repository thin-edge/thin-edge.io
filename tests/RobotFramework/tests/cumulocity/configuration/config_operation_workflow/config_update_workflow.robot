*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Suite Setup
Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:configuration    theme:workflows


*** Variables ***
${DEVICE_SN}        None
${CONFIG_URL}       None


*** Test Cases ***
Default Workflow
    Update Config And Verify

Workflow Override With Custom Set Script
    # Workflow with custom set script
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/config_update_custom_set.toml
    ...    /etc/tedge/operations/config_update.toml

    Update Config And Verify

Legacy Workflow
    # Legacy workflow definition
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/legacy_config_update.toml
    ...    /etc/tedge/operations/config_update.toml

    Update Config And Verify


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup    skip_bootstrap=False
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    # Test config
    Execute Command    touch /etc/tedge/test.conf
    Execute Command    echo "original config" >> /etc/tedge/test.conf

    # Config plugin with the test config entry
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/tedge-configuration-plugin.toml
    ...    /etc/tedge/plugins/tedge-configuration-plugin.toml
    Should Contain Supported Configuration Types    test-conf

Custom Suite Setup
    ${CONFIG_URL}    Cumulocity.Create Inventory Binary
    ...    test-conf
    ...    test-conf
    ...    contents=updated config
    Set Suite Variable    ${CONFIG_URL}

Update Config And Verify
    ${operation}    Cumulocity.Set Configuration    test-conf    url=${CONFIG_URL}
    Operation Should Be SUCCESSFUL    ${operation}    timeout=30
    File Should Contain    /etc/tedge/test.conf    updated config

File Should Contain
    [Arguments]    ${file_path}    ${expected_content}
    ${output}    Execute Command    cat ${file_path}
    Should Contain    ${output}    ${expected_content}
