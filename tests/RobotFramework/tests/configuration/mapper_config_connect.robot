*** Settings ***
Documentation       Test tedge connect displays mapper config file location correctly

Resource            ../../resources/common.resource
Resource            ./mapper_migration.resource
Library             ThinEdgeIO

Suite Setup         Migration Suite Setup
Suite Teardown      Migration Suite Teardown
Test Setup          Prepare Clean Environment
Test Teardown       Test Cleanup

Test Tags           theme:configuration    theme:mapper    theme:connect


*** Test Cases ***
Connection Succeeds With Migrated Config
    [Documentation]    Verify actual connection works with migrated config
    # Setup and migrate
    Migrate Cloud Config    c8y

    # Connect (actual connection, not test mode)
    Execute Command    sudo tedge reconnect c8y

    # Verify: Mapper service is running
    Service Health Status Should Be Up    tedge-mapper-c8y

Connect Output Shows Legacy Config Path Before Migration
    [Documentation]    Verify tedge connect shows tedge.toml path for non-migrated config
    # Setup without migration
    Setup Test Config    c8y    test.c8y.io

    # Run connect in test mode
    ${output}=    Execute Command    tedge connect c8y --test    stderr=${True}    stdout=${False}

    # Verify: Shows tedge.toml as config source
    Should Contain    ${output}    mapper configuration file
    Should Contain    ${output}    /etc/tedge/tedge.toml

Connect Output Shows Migrated Config Path After Migration
    [Documentation]    Verify tedge connect shows mapper config file path after migration
    # Setup and migrate
    Setup Test Config    c8y    test.c8y.io
    Migrate Cloud Config    c8y

    # Run connect in test mode
    ${output}=    Execute Command    tedge connect c8y --test    stderr=${True}    stdout=${False}

    # Verify: Shows mapper config file as source
    Should Contain    ${output}    mapper configuration file
    Should Contain    ${output}    /etc/tedge/mappers/c8y.toml

Connect Output Shows Profile Config Path
    [Documentation]    Verify tedge connect shows correct path for profiled config
    # Setup and migrate profiled config
    Setup Test Config    c8y    production.c8y.io    profile=production
    Migrate Cloud Config    c8y

    # Run connect in test mode with profile
    # Test should fail as we're not connected
    ${output}=    Execute Command
    ...    tedge connect c8y --profile production --test
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}

    # Verify: Shows profile-specific mapper config file
    Should Contain    ${output}    mapper configuration file
    Should Contain    ${output}    /etc/tedge/mappers/c8y.d/production.toml

Connect Test Mode Works With Migrated Config
    [Documentation]    Verify test mode validates configuration correctly
    # Setup and migrate
    Setup Test Config    c8y    test.c8y.io
    Migrate Cloud Config    c8y

    # Run connect in test mode - should succeed even with fake URL
    ${output}=    Execute Command    tedge connect c8y --test    stderr=${True}    stdout=${False}

    # Verify: Test mode completes and shows config
    Should Contain    ${output}    mapper configuration file
    Should Contain    ${output}    /etc/tedge/mappers/c8y.toml

Azure Connect Shows Migrated Config Path
    [Documentation]    Spot-check: Verify Azure connect shows correct config path
    # Setup and migrate
    Setup Test Config    az    test.azure.com
    Migrate Cloud Config    az

    # Run connect in test mode
    ${output}=    Execute Command    tedge connect az --test    exp_exit_code=!0    stderr=${True}    stdout=${False}

    # Verify: Shows Azure mapper config file
    Should Contain    ${output}    mapper configuration file
    Should Contain    ${output}    /etc/tedge/mappers/az.toml

AWS Connect Shows Migrated Config Path
    [Documentation]    Spot-check: Verify AWS connect shows correct config path
    # Setup and migrate
    Setup Test Config    aws    test.amazonaws.com
    Migrate Cloud Config    aws

    # Run connect in test mode
    ${output}=    Execute Command    tedge connect aws --test    exp_exit_code=!0    stderr=${True}    stdout=${False}

    # Verify: Shows AWS mapper config file
    Should Contain    ${output}    mapper configuration file
    Should Contain    ${output}    /etc/tedge/mappers/aws.toml
