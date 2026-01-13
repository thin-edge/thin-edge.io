*** Settings ***
Documentation       Test tedge config upgrade command functionality

Resource            ../../resources/common.resource
Resource            ./mapper_migration.resource
Library             ThinEdgeIO

Suite Setup         Migration Suite Setup
Suite Teardown      Migration Suite Teardown
Test Setup          Prepare Clean Environment
Test Teardown       Test Cleanup

Test Tags           theme:configuration    theme:mapper


*** Test Cases ***
Migrate C8y Config Default Profile
    [Documentation]    Verify migration creates correct file for C8y default profile
    # Setup: Configure C8y URL in tedge.toml
    Setup Test Config    c8y    test.c8y.io

    # Execute migration
    Migrate Cloud Configs

    # Verify: Config file created
    Verify Mapper Config File Exists    c8y
    Verify Mapper Config File Contains    c8y    url    test.c8y.io

    # Verify: tedge.toml cleaned up (no [c8y] section)
    Verify Tedge Toml Does Not Contain Section    c8y

Migrate C8y Named Profile
    [Documentation]    Verify migration creates correct directory structure for named profiles
    # Setup: Configure named profile
    Setup Test Config    c8y    production.c8y.io    profile=production

    # Execute migration
    Migrate Cloud Configs

    # Verify: Profile config file created in .d directory
    Verify Mapper Config File Exists    c8y    profile=production
    Verify Mapper Config File Contains    c8y    url    production.c8y.io    profile=production

    # Verify: Default profile file should not exist as there are no configurations for that
    Verify Mapper Config File Does Not Exist    c8y

    # Verify: tedge.toml cleaned up
    Verify Tedge Toml Does Not Contain Section    c8y.profiles.production

Migrate Multiple C8y Profiles
    [Documentation]    Verify migration handles multiple profiles correctly
    # Setup: Configure default and two named profiles
    Setup Test Config    c8y    default.c8y.io
    Setup Test Config    c8y    prod.c8y.io    profile=prod
    Setup Test Config    c8y    test.c8y.io    profile=test

    # Execute migration
    Migrate Cloud Configs

    # Verify: All three config files created
    Verify Mapper Config File Exists    c8y
    Verify Mapper Config File Contains    c8y    url    default.c8y.io

    Verify Mapper Config File Exists    c8y    profile=prod
    Verify Mapper Config File Contains    c8y    url    prod.c8y.io    profile=prod

    Verify Mapper Config File Exists    c8y    profile=test
    Verify Mapper Config File Contains    c8y    url    test.c8y.io    profile=test

Migration Is Idempotent
    [Documentation]    Verify migration can be run multiple times safely
    # Setup and first migration
    Setup Test Config    c8y    test.c8y.io
    Migrate Cloud Configs

    # Run migration again
    Migrate Cloud Configs

    # Verify: Still works correctly
    Verify Mapper Config File Exists    c8y
    Verify Mapper Config File Contains    c8y    url    test.c8y.io

Migration Preserves C8y Topics Config
    [Documentation]    Verify migration preserves topics configuration
    # Setup: Configure C8y with topics
    Setup Test Config    c8y    test.c8y.io
    Execute Command    sudo tedge config set c8y.topics "te/+/+/+/+/e/+"

    # Execute migration
    Migrate Cloud Configs

    # Verify: Topics preserved in migrated config
    Verify Mapper Config File Contains Pattern    c8y    topics = ["te/+/+/+/+/e/+"]

Migration With Empty C8y Config
    [Documentation]    Verify migration only creates directories if config exists
    # Setup: Just set a minimal config
    Execute Command    sudo tedge config unset c8y.url

    # Execute migration
    Migrate Cloud Configs

    # Verify: File not created (since it would have been empty)
    Verify Mapper Config File Does Not Exist    c8y

Migration With No Write Permission On Mappers Directory
    [Documentation]    Verify migration handles permission errors gracefully on mappers directory
    # Setup
    Setup Test Config    c8y    test.c8y.io

    # Create mappers directory but remove write permissions
    Execute Command    sudo mkdir -p /etc/tedge/mappers
    Execute Command    sudo chmod 555 /etc/tedge/mappers

    # Attempt migration - should fail with permission error referencing the path
    ${result}=    Execute Command
    ...    sudo -u tedge tedge config upgrade
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}
    Should Contain    ${result}    Permission denied
    Should Contain    ${result}    /etc/tedge/mappers

    # Cleanup: Restore permissions
    Execute Command    sudo chmod 755 /etc/tedge/mappers

Migration With Write Protected Tedge Toml
    [Documentation]    Verify migration handles write-protected tedge.toml
    # Setup
    Setup Test Config    c8y    test.c8y.io

    # Ensure mappers config dir exists (it might not if this test is run on its own)
    Execute Command    sudo -u tedge mkdir -p /etc/tedge/mappers
    Execute Command    sudo chmod 755 /etc/tedge/mappers

    # Make tedge directory read-only
    Execute Command    sudo chmod 555 /etc/tedge

    # Attempt migration - should fail with permission error referencing tedge.toml
    ${result}=    Execute Command
    ...    sudo -u tedge tedge config upgrade
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}
    Should Contain    ${result}    Permission denied
    Should Contain    ${result}    /etc/tedge/tedge.toml

    # Cleanup: Restore permissions
    Execute Command    sudo chmod 755 /etc/tedge

Migrate Azure Config Default Profile
    [Documentation]    Spot-check: Verify migration works for Azure
    # Setup: Configure Azure URL
    Setup Test Config    az    test.azure.com

    # Execute migration
    Migrate Cloud Configs

    # Verify: Config file created
    Verify Mapper Config File Exists    az
    Verify Mapper Config File Contains    az    url    test.azure.com

    # Verify: tedge.toml cleaned up
    Verify Tedge Toml Does Not Contain Section    az

Migrate AWS Config Default Profile
    [Documentation]    Spot-check: Verify migration works for AWS
    # Setup: Configure AWS URL
    Setup Test Config    aws    test.amazonaws.com

    # Execute migration
    Migrate Cloud Configs

    # Verify: Config file created
    Verify Mapper Config File Exists    aws
    Verify Mapper Config File Contains    aws    url    test.amazonaws.com

    # Verify: tedge.toml cleaned up
    Verify Tedge Toml Does Not Contain Section    aws

Migration Fails When Tedge Dir Not Writable And Mappers Dir Missing
    [Documentation]    Verify clear error when /etc/tedge is read-only and mappers dir doesn't exist
    # Setup config
    Setup Test Config    c8y    test.c8y.io

    # Ensure mappers dir doesn't exist
    Execute Command    sudo rm -rf /etc/tedge/mappers

    # Make /etc/tedge read-only (can't create mappers dir)
    Execute Command    sudo chmod 555 /etc/tedge

    # Attempt migration - should fail
    ${result}=    Execute Command
    ...    sudo -u tedge tedge config upgrade
    ...    exp_exit_code=!0
    ...    stderr=${True}
    ...    stdout=${False}
    Should Contain    ${result}    Permission denied
    # The error should talk about how `/etc/tedge/mappers` can't be created
    Should Contain    ${result}    /etc/tedge/mappers

    # Cleanup: Restore permissions
    Execute Command    sudo chmod 755 /etc/tedge

Migrate Multiple Clouds And Profiles
    [Documentation]    Verify migration handles multiple cloud and their profiles simltaneously
    # Setup: Configure default and two named profiles
    Setup Test Config    c8y    default.c8y.io
    Setup Test Config    c8y    prod.c8y.io    profile=prod
    Setup Test Config    c8y    test.c8y.io    profile=test

    Setup Test Config    az    default.azure.com
    Setup Test Config    az    prod.azure.com    profile=prod

    Setup Test Config    aws    default.amazonaws.com
    Setup Test Config    aws    test.amazonaws.com    profile=test

    # Execute migration
    Migrate Cloud Configs

    # Verify: All three config files created
    Verify Mapper Config File Exists    c8y
    Verify Mapper Config File Contains    c8y    url    default.c8y.io

    Verify Mapper Config File Exists    c8y    profile=prod
    Verify Mapper Config File Contains    c8y    url    prod.c8y.io    profile=prod

    Verify Mapper Config File Exists    c8y    profile=test
    Verify Mapper Config File Contains    c8y    url    test.c8y.io    profile=test

    Verify Mapper Config File Exists    az
    Verify Mapper Config File Contains    az    url    default.azure.com
    Verify Mapper Config File Exists    az    profile=prod
    Verify Mapper Config File Contains    az    url    prod.azure.com    profile=prod

    Verify Mapper Config File Exists    aws
    Verify Mapper Config File Contains    aws    url    default.amazonaws.com
    Verify Mapper Config File Exists    aws    profile=test
    Verify Mapper Config File Contains    aws    url    test.amazonaws.com    profile=test
