*** Settings ***
Documentation       Test error handling during mapper config migration
...
...                 This suite tests various failure scenarios that can occur during
...                 the mapper config migration process, particularly focusing on partial
...                 failures and transactional behavior.

Resource            ../../resources/common.resource
Resource            ./mapper_migration.resource
Library             ThinEdgeIO

Suite Setup         Migration Suite Setup
Suite Teardown      Migration Suite Teardown
Test Setup          Prepare Clean Environment
Test Teardown       Test Cleanup

Test Tags           theme:configuration    theme:mapper    theme:error-handling


*** Test Cases ***
Migration Fails When C8y Default Profile Directory Cannot Be Created
    [Documentation]    Verify migration fails gracefully when one mapper directory cannot be created
    ...
    ...    This tests the scenario where we have multiple profiles but one directory
    ...    fails to be created. We want to ensure the error message is clear and that
    ...    /etc/tedge/tedge.toml remains unchanged.

    # Setup: Configure default and named profile
    Setup Test Config    c8y    default.c8y.io
    Setup Test Config    c8y    prod.c8y.io    profile=prod

    # Backup original tedge.toml state to verify it's not corrupted on failure
    ${original_tedge_toml}=    Execute Command    cat /etc/tedge/tedge.toml

    # Create mappers directory and make it read-only so subdirs can't be created
    Execute Command    sudo mkdir -p /etc/tedge/mappers
    Execute Command    sudo chmod 555 /etc/tedge/mappers

    # Attempt migration - should fail
    ${result}=    Execute Command
    ...    sudo -u tedge tedge config upgrade
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}
    Should Contain    ${result}    Permission denied
    Should Contain    ${result}    /etc/tedge/mappers

    # Verify: tedge.toml should be unchanged (configs still present)
    ${current_tedge_toml}=    Execute Command    cat /etc/tedge/tedge.toml
    Should Be Equal    ${original_tedge_toml}    ${current_tedge_toml}
    Verify Tedge Toml Contains Section    c8y
    Verify Tedge Toml Contains Section    c8y.profiles.prod

    # Verify: No mapper config files were created
    Verify Mapper Config File Does Not Exist    c8y
    Verify Mapper Config File Does Not Exist    c8y    profile=prod

    # Cleanup: Restore permissions for teardown
    Execute Command    sudo chmod 755 /etc/tedge/mappers

Migration Fails When First Profile Succeeds But Second Profile Fails
    [Documentation]    Verify behavior when migration partially succeeds
    ...
    ...    This is a critical test: if we successfully write to /etc/tedge/mappers/c8y
    ...    but fail to write to /etc/tedge/mappers/c8y.prod, what happens?
    ...    - Is the first file left behind?
    ...    - Is tedge.toml partially cleaned up?
    ...    - Is the error message clear about which operation failed?

    # Setup: Configure default and named profile
    Setup Test Config    c8y    default.c8y.io
    Setup Test Config    c8y    prod.c8y.io    profile=prod

    # Backup original state
    ${original_tedge_toml}=    Execute Command    cat /etc/tedge/tedge.toml

    # Pre-create the mappers directory structure
    Execute Command    sudo -u tedge mkdir -p /etc/tedge/mappers
    Execute Command    sudo -u tedge chmod 755 /etc/tedge/mappers

    # Create a read-only directory that will block the named profile
    # Note: We create c8y.prod directory but make it read-only so the config file can't be written
    Execute Command    sudo -u tedge mkdir -p /etc/tedge/mappers/c8y.prod
    Execute Command    sudo -u tedge chmod 555 /etc/tedge/mappers/c8y.prod

    # Attempt migration
    ${result}=    Execute Command
    ...    sudo -u tedge tedge config upgrade
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}
    ...    retries=0
    Should Contain    ${result}    Permission denied
    Should Contain    ${result}    /etc/tedge/mappers/c8y.prod

    # 1. Verify the named profile config was NOT created
    Verify Mapper Config File Does Not Exist    c8y    profile=prod

    # 2. Most important: tedge.toml should still contain the original config
    #    (migration should be atomic - all or nothing)
    Verify Tedge Toml Contains Section    c8y
    Verify Tedge Toml Contains Section    c8y.profiles.prod

    # Cleanup: Restore permissions
    Execute Command    sudo chmod 755 /etc/tedge/mappers/c8y.prod

Migration Fails When Multiple Clouds Have Permission Issues
    [Documentation]    Test migration with multiple clouds where some fail
    ...
    ...    Setup: c8y (default + profile), az (default), aws (default)
    ...    Failure: Block c8y.prod and aws directories
    ...    Expected: Nothing migrated, all configs remain in tedge.toml

    # Setup multiple clouds
    Setup Test Config    c8y    default.c8y.io
    Setup Test Config    c8y    prod.c8y.io    profile=prod
    Setup Test Config    az    default.azure.com
    Setup Test Config    aws    default.amazonaws.com

    ${original_tedge_toml}=    Execute Command    cat /etc/tedge/tedge.toml

    # Create mapper directory structure with strategic blocks
    Execute Command    sudo mkdir -p /etc/tedge/mappers/c8y.prod
    Execute Command    sudo chmod 555 /etc/tedge/mappers/c8y.prod
    Execute Command    sudo mkdir -p /etc/tedge/mappers/aws
    Execute Command    sudo chmod 555 /etc/tedge/mappers/aws

    # Attempt migration
    ${result}=    Execute Command
    ...    sudo -u tedge tedge config upgrade
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}
    Should Contain    ${result}    Permission denied

    # Verify all config remains in tedge.toml
    Verify Tedge Toml Contains Section    c8y
    Verify Tedge Toml Contains Section    c8y.profiles.prod
    Verify Tedge Toml Contains Section    az
    Verify Tedge Toml Contains Section    aws

    # Cleanup
    Execute Command    sudo chmod 755 /etc/tedge/mappers/c8y.prod
    Execute Command    sudo chmod 755 /etc/tedge/mappers/aws

Migration Recovers From Interrupted Previous Attempt
    [Documentation]    Test recovery when a previous migration was interrupted
    ...
    ...    Scenario: A previous migration partially succeeded (created some mapper
    ...    config files) but was interrupted. Running migration again should:
    ...    - Detect existing mapper configs
    ...    - Complete the migration (idempotent behavior)
    ...    - Clean up tedge.toml appropriately

    # Setup
    Setup Test Config    c8y    default.c8y.io
    Setup Test Config    c8y    prod.c8y.io    profile=prod

    # Simulate partial migration: manually create default profile config
    Execute Command    sudo -u tedge mkdir -p /etc/tedge/mappers/c8y    timeout=0
    Execute Command
    ...    sudo -u tedge sh -c 'echo "url \= \\"default.c8y.io\\"" > /etc/tedge/mappers/c8y/tedge.toml'
    ...    retries=0
    ...    timeout=0

    # tedge.toml still has both configs
    Verify Tedge Toml Contains Section    c8y
    Verify Tedge Toml Contains Section    c8y.profiles.prod

    # Now run full migration
    Migrate Cloud Configs

    # Verify: Both mapper configs exist now
    Verify Mapper Config File Exists    c8y
    Verify Mapper Config File Contains    c8y    url    default.c8y.io
    Verify Mapper Config File Exists    c8y    profile=prod
    Verify Mapper Config File Contains    c8y    url    prod.c8y.io    profile=prod

    # Verify: tedge.toml cleaned up
    Verify Tedge Toml Does Not Contain Section    c8y
    Verify Tedge Toml Does Not Contain Section    c8y.profiles.prod

Migration Handles Corrupted Tedge Toml
    [Documentation]    Test migration behavior with malformed tedge.toml
    ...
    ...    If tedge.toml is corrupted, the migration should fail with a clear
    ...    error message and not create any mapper config files.

    Execute Command    sudo cp /etc/tedge/tedge.toml /etc/tedge/tedge.toml.bak

    # Create a corrupted tedge.toml
    Execute Command    sudo sh -c 'echo "[c8y" > /etc/tedge/tedge.toml'

    # Attempt migration
    ${result}=    Execute Command
    ...    sudo -u tedge tedge config upgrade
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}

    # Should report TOML parsing error
    Should Contain Any    ${result}    expected    invalid    parse    TOML

    # No mapper configs should be created
    Execute Command    test ! -d /etc/tedge/mappers/c8y    retries=0    timeout=0
    [Teardown]    Restore Tedge Toml Backup

Migration Creates Parent Directories With Correct Permissions
    [Documentation]    Verify that created directories have correct ownership and permissions
    ...
    ...    Even when testing error handling, we should verify that successful
    ...    directory creation uses appropriate permissions (755) and ownership (tedge:tedge).

    Setup Test Config    c8y    test.c8y.io

    # Ensure mappers directory doesn't exist
    Execute Command    sudo rm -rf /etc/tedge/mappers

    # Run migration
    Migrate Cloud Configs

    # Verify directory permissions
    ${mappers_perms}=    Execute Command    stat -c '%a' /etc/tedge/mappers
    Should Match Regexp    ${mappers_perms}    ^7[0-9][0-9]$

    ${c8y_perms}=    Execute Command    stat -c '%a' /etc/tedge/mappers/c8y
    Should Match Regexp    ${c8y_perms}    ^7[0-9][0-9]$

    # Verify file permissions (should be 644)
    ${file_perms}=    Execute Command    stat -c '%a' /etc/tedge/mappers/c8y/tedge.toml
    Should Be Equal    ${file_perms.strip()}    644

    # Verify ownership
    ${dir_owner}=    Execute Command    stat -c '%U:%G' /etc/tedge/mappers/c8y
    Should Contain    ${dir_owner}    tedge

Migration Error Messages Are User Friendly
    [Documentation]    Verify error messages are actionable and clear
    ...
    ...    When migration fails, users should get clear guidance on:
    ...    - What operation failed
    ...    - Which file/directory caused the issue
    ...    - What permissions are needed
    ...    - How to fix the problem

    Setup Test Config    c8y    test.c8y.io

    # Create a permission problem
    Execute Command    sudo mkdir -p /etc/tedge/mappers
    Execute Command    sudo chmod 000 /etc/tedge/mappers

    # Attempt migration and check error quality
    ${result}=    Execute Command
    ...    sudo -u tedge tedge config upgrade
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}

    # Error message should contain:
    Should Contain    ${result}    Permission denied
    # The actual path that failed
    Should Contain    ${result}    /etc/tedge/mappers

    # Cleanup
    Execute Command    sudo chmod 755 /etc/tedge/mappers


*** Keywords ***
Restore Tedge Toml Backup
    Execute Command    sudo cp /etc/tedge/tedge.toml.bak /etc/tedge/tedge.toml
    Test Cleanup
