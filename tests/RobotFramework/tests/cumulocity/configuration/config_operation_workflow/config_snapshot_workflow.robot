*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:configuration    theme:workflows


*** Variables ***
${DEVICE_SN}        None


*** Test Cases ***
Default Workflow
    ${original_config}    Execute Command    cat /etc/tedge/operations/config_snapshot.toml
    ${original_template}    Execute Command    cat /etc/tedge/operations/config_snapshot.toml.template

    Should Be Equal    ${original_config}    ${original_template}

    Snapshot Config And Verify

Workflow Override With Custom Step
    ${original_config}    Execute Command    cat /etc/tedge/operations/config_snapshot.toml
    ${original_template}    Execute Command    cat /etc/tedge/operations/config_snapshot.toml.template

    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/config_snapshot_custom_step.toml
    ...    /etc/tedge/operations/config_snapshot.toml

    ${updated_config}    Execute Command    cat /etc/tedge/operations/config_snapshot.toml
    Should Not Be Equal    ${original_config}    ${updated_config}

    Snapshot Config And Verify
    File Should Contain    /tmp/config_snapshot_workflow.log    custom-snapshot-workflow

    Execute Command    systemctl restart tedge-agent

    ${config_after_restart}    Execute Command    cat /etc/tedge/operations/config_snapshot.toml
    ${template_after_restart}    Execute Command    cat /etc/tedge/operations/config_snapshot.toml.template

    Should Be Equal    ${config_after_restart}    ${updated_config}
    Should Be Equal    ${template_after_restart}    ${original_template}
    Should Not Be Equal    ${config_after_restart}    ${template_after_restart}

Legacy Workflow
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/legacy_config_snapshot.toml
    ...    /etc/tedge/operations/config_snapshot.toml

    Snapshot Config And Verify


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup    skip_bootstrap=False
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Execute Command    printf 'snapshot workflow test\n' > /etc/tedge/test.conf
    Execute Command    rm -f /tmp/config_snapshot_workflow.log

    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/tedge-configuration-plugin.toml
    ...    /etc/tedge/plugins/tedge-configuration-plugin.toml
    Should Contain Supported Configuration Types    test-conf

Snapshot Config And Verify
    ${operation}    Cumulocity.Get Configuration    test-conf
    Operation Should Be SUCCESSFUL    ${operation}    timeout=30

File Should Contain
    [Arguments]    ${file_path}    ${expected_content}
    ${output}    Execute Command    cat ${file_path}
    Should Contain    ${output}    ${expected_content}
