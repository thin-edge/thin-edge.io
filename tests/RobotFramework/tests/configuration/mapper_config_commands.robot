*** Settings ***
Documentation       Test tedge config command behavior with migrated mapper configurations

Resource            ../../resources/common.resource
Resource            ./mapper_migration.resource
Library             ThinEdgeIO

Suite Setup         Migration Suite Setup
Suite Teardown      Migration Suite Teardown
Test Setup          Prepare Clean Environment
Test Teardown       Test Cleanup

Test Tags           theme:configuration    theme:mapper    theme:cli


*** Test Cases ***
Tedge Config Get Works With Migrated C8y Config
    [Documentation]    Verify tedge config get returns values from migrated config
    # Setup and migrate
    Setup Test Config    c8y    test.c8y.io
    Migrate Cloud Configs

    # Verify: Config get works
    ${url}=    Execute Command    tedge config get c8y.url
    Should Be Equal As Strings    ${url}    test.c8y.io\n

Tedge Config List Shows Migrated C8y Config
    [Documentation]    Verify tedge config list displays migrated configuration
    # Setup and migrate
    Setup Test Config    c8y    test.c8y.io
    Execute Command    sudo tedge config set c8y.topics "te/+/+/+/+/e/+"
    Migrate Cloud Configs

    # Verify: Config list shows migrated values
    ${list}=    Execute Command    tedge config list
    Should Contain    ${list}    c8y.url=test.c8y.io
    Should Contain    ${list}    c8y.topics

Tedge Config Get Works With C8y Profile
    [Documentation]    Verify tedge config get works with profiled config
    # Setup and migrate profiled config
    Setup Test Config    c8y    production.c8y.io    profile=production
    Migrate Cloud Configs

    # Verify: Profile config get works
    ${url}=    Execute Command    tedge config get c8y.profiles.production.url
    Should Be Equal As Strings    ${url}    production.c8y.io\n

Tedge Config Set Works With Migrated C8y Config
    [Documentation]    Verify tedge config set updates the migrated config file
    # Setup and migrate
    Setup Test Config    c8y    test.c8y.io
    Migrate Cloud Configs

    # Set a new value
    Execute Command    sudo tedge config set c8y.url updated.c8y.io

    # Verify: Value updated in mapper config file
    Verify Mapper Config File Contains    c8y    url    updated.c8y.io

    # Verify: Config get returns new value
    ${url}=    Execute Command    tedge config get c8y.url
    Should Be Equal As Strings    ${url}    updated.c8y.io\n

Tedge Config Unset Works With Migrated C8y Config
    [Documentation]    Verify tedge config unset removes values from migrated config
    # Setup with topics and migrate
    Setup Test Config    c8y    test.c8y.io
    Execute Command    sudo tedge config set c8y.topics "te/+/+/+/+/e/+"
    Migrate Cloud Configs

    # Unset topics
    Execute Command    sudo tedge config unset c8y.topics

    # Verify: Topics removed from mapper config file
    ${path}=    Get Expected Mapper Config Path    c8y
    ${content}=    Execute Command    cat ${path}
    Should Not Contain    ${content}    topics =

Tedge Config Add Works With Migrated C8y Config
    [Documentation]    Verify tedge config add updates array values in migrated config
    # Setup and migrate
    Setup Test Config    c8y    test.c8y.io
    Migrate Cloud Configs

    # Add smartrest templates
    Execute Command    sudo tedge config add c8y.smartrest.templates template1
    Execute Command    sudo tedge config add c8y.smartrest.templates template2

    # Verify: Templates added to mapper config file
    ${path}=    Get Expected Mapper Config Path    c8y
    ${content}=    Execute Command    cat ${path}
    Should Contain    ${content}    template1
    Should Contain    ${content}    template2

Tedge Config Remove Works With Migrated C8y Config
    [Documentation]    Verify tedge config remove updates array values in migrated config
    # Setup with templates and migrate
    Setup Test Config    c8y    test.c8y.io
    Execute Command    sudo tedge config add c8y.smartrest.templates template1
    Execute Command    sudo tedge config add c8y.smartrest.templates template2
    Migrate Cloud Configs

    # Remove one template
    Execute Command    sudo tedge config remove c8y.smartrest.templates template1

    # Verify: Template removed from mapper config file
    ${path}=    Get Expected Mapper Config Path    c8y
    ${content}=    Execute Command    cat ${path}
    Should Not Contain    ${content}    template1
    Should Contain    ${content}    template2

Tedge Config Set Works With C8y Profile
    [Documentation]    Verify tedge config set works with profiled config
    # Setup and migrate profiled config
    Setup Test Config    c8y    production.c8y.io    profile=production
    Migrate Cloud Configs

    # Set a new value for profile
    Execute Command    sudo tedge config set c8y.profiles.production.url updated.c8y.io

    # Verify: Value updated in profile config file
    Verify Mapper Config File Contains    c8y    url    updated.c8y.io    profile=production

Tedge Config Set Works For Non-Cloud Settings
    [Documentation]    Verify tedge config set still works for non-cloud settings
    # Setup and migrate
    Setup Test Config    c8y    test.c8y.io
    Migrate Cloud Configs

    # Non-cloud config should still work
    Execute Command    sudo tedge config set device.type custom-device
    ${device_type}=    Execute Command    tedge config get device.type
    Should Be Equal As Strings    ${device_type}    custom-device\n

    # Verify: device.type is in tedge.toml, not mapper config
    Verify Tedge Toml Contains Pattern    [device]\ntype = "custom-device"

Tedge Config Set Works Before And After Migration
    [Documentation]    Verify config operations work both before and after migration
    # Setup before migration
    Setup Test Config    c8y    before-migration.c8y.io

    # Verify: Works before migration
    ${url}=    Execute Command    tedge config get c8y.url
    Should Be Equal As Strings    ${url}    before-migration.c8y.io\n

    # Migrate
    Migrate Cloud Configs

    # Verify: Still works after migration
    Execute Command    sudo tedge config set c8y.url after-migration.c8y.io
    ${url}=    Execute Command    tedge config get c8y.url
    Should Be Equal As Strings    ${url}    after-migration.c8y.io\n

    # Verify: Updated value in mapper config file
    Verify Mapper Config File Contains    c8y    url    after-migration.c8y.io

Tedge Config Works With Multiple Migrated Clouds
    [Documentation]    Verify config commands work when multiple clouds are migrated
    # Setup and migrate c8y and az
    Setup Test Config    c8y    test.c8y.io
    Setup Test Config    az    test.azure.com
    Migrate Cloud Configs

    # Verify: Both can be read
    ${c8y_url}=    Execute Command    tedge config get c8y.url
    ${az_url}=    Execute Command    tedge config get az.url
    Should Be Equal As Strings    ${c8y_url}    test.c8y.io\n
    Should Be Equal As Strings    ${az_url}    test.azure.com\n

    # Verify: Both can be updated
    Execute Command    sudo tedge config set c8y.url updated-c8y.c8y.io
    Execute Command    sudo tedge config set az.url updated-az.azure.com

    Verify Mapper Config File Contains    c8y    url    updated-c8y.c8y.io
    Verify Mapper Config File Contains    az    url    updated-az.azure.com

Tedge Config Set Works With Migrated Azure Config
    [Documentation]    Spot-check: Verify config set works for migrated Azure config
    # Setup and migrate
    Setup Test Config    az    test.azure.com
    Migrate Cloud Configs

    # Set new value
    Execute Command    sudo tedge config set az.url updated.azure.com

    # Verify: Updated in mapper config file
    Verify Mapper Config File Contains    az    url    updated.azure.com

Tedge Config Set Works With Migrated AWS Config
    [Documentation]    Spot-check: Verify config set works for migrated AWS config
    # Setup and migrate
    Setup Test Config    aws    test.amazonaws.com
    Migrate Cloud Configs

    # Set new value
    Execute Command    sudo tedge config set aws.url updated.amazonaws.com

    # Verify: Updated in mapper config file
    Verify Mapper Config File Contains    aws    url    updated.amazonaws.com
