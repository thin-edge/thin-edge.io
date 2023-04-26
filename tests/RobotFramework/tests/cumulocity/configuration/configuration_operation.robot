*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:configuration
Suite Setup    Custom Setup
Test Teardown    Get Logs
Test Setup    Test Setup

# Note: Use larger timeout (120s instead of 30s default) for operation
# assertions to allow for cases where the c8y jwt token request times out
# as the retry mechanism will wait 60 seconds before requesting a new jwt

*** Variables ***
${DEFAULT_CONFIG}    c8y-configuration-plugin

*** Test Cases ***

Set configuration when file exists
    ${config_url}=    Cumulocity.Create Inventory Binary    config1    CONFIG1    file=${CURDIR}/config1-version2.json
    ${operation}=    Cumulocity.Set Configuration    CONFIG1    url=${config_url}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    ${target_contents}=    Execute Command    cat /etc/config1.json
    Should Be Equal    ${target_contents}    {"name":"configuration1","version":2}
    ${FILE_MODE_OWNERSHIP}=    Execute Command    stat -c '%a %U:%G' /etc/config1.json    strip=${True}
    # Note: File permission will not change if the file already exists
    Should Be Equal    ${FILE_MODE_OWNERSHIP}    644 root:root

Set configuration when file does not exist
    Execute Command    rm -f /etc/config1.json
    ${config_url}=    Cumulocity.Create Inventory Binary    config1    CONFIG1    file=${CURDIR}/config1-version2.json
    ${operation}=    Cumulocity.Set Configuration    CONFIG1    url=${config_url}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    ${target_contents}=    Execute Command    cat /etc/config1.json
    Should Be Equal    ${target_contents}    {"name":"configuration1","version":2}
    ${FILE_MODE_OWNERSHIP}=    Execute Command    stat -c '%a %U:%G' /etc/config1.json    strip=${True}
    Should Be Equal    ${FILE_MODE_OWNERSHIP}    640 tedge:tedge

Set configuration with broken url
    ${operation}=    Cumulocity.Set Configuration    CONFIG1    url=invalid://hellö.zip
    ${operation}=    Operation Should Be FAILED    ${operation}    timeout=120
    ${target_contents}=    Execute Command    cat /etc/config1.json
    Should Be Equal    ${target_contents}    {"name":"configuration1"}

Get configuration
    ${operation}=    Cumulocity.Get Configuration    CONFIG1
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    ${events}=    Cumulocity.Device Should Have Event/s    minimum=1    maximum=1    type=CONFIG1
    ${uploaded_contents}=    Cumulocity.Event Should Have An Attachment    ${events[0]["id"]}
    ${target_contents}=    Execute Command    cat /etc/config1.json
    Should Be Equal    ${target_contents}    ${uploaded_contents.decode("utf8")}
    Should Be Equal    ${target_contents}    {"name":"configuration1"}

Get non existent configuration file
    Execute Command    rm -f /etc/config1.json
    File Should Not Exist    /etc/config1.json
    ${operation}=    Cumulocity.Get Configuration    CONFIG1
    Operation Should Be FAILED    ${operation}    failure_reason=.*No such file or directory.*

Get non existent configuration type
    ${operation}=    Cumulocity.Get Configuration    unknown_config
    Operation Should Be FAILED    ${operation}    failure_reason=.*requested config_type unknown_config is not defined in the plugin configuration file.*

Update configuration plugin config via cloud
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1
    ${config_url}=    Cumulocity.Create Inventory Binary    c8y-configuration-plugin    ${DEFAULT_CONFIG}    file=${CURDIR}/c8y-configuration-plugin-updated.toml
    ${operation}=    Cumulocity.Set Configuration    ${DEFAULT_CONFIG}    url=${config_url}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1    Config@2.0.0

Modify configuration plugin config via local filesystem modify inplace
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1
    Execute Command    sed -i 's/CONFIG1/CONFIG3/g' /etc/tedge/c8y/c8y-configuration-plugin.toml
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG3

Modify configuration plugin config via local filesystem overwrite
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1
    ${NEW_CONFIG}=    Execute Command    sed 's/CONFIG1/CONFIG3/g' /etc/tedge/c8y/c8y-configuration-plugin.toml
    Execute Command    echo "${NEW_CONFIG}" > /etc/tedge/c8y/c8y-configuration-plugin.toml
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG3

Update configuration plugin config via local filesystem copy
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1
    Transfer To Device    ${CURDIR}/c8y-configuration-plugin-updated.toml    /etc/tedge/c8y/
    Execute Command    cp /etc/tedge/c8y/c8y-configuration-plugin-updated.toml /etc/tedge/c8y/c8y-configuration-plugin.toml
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1    Config@2.0.0

Update configuration plugin config via local filesystem move (different directory)
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1
    Transfer To Device    ${CURDIR}/c8y-configuration-plugin-updated.toml    /etc/
    Execute Command    mv /etc/c8y-configuration-plugin-updated.toml /etc/tedge/c8y/c8y-configuration-plugin.toml
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1    Config@2.0.0

Update configuration plugin config via local filesystem move (same directory)
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1
    Transfer To Device    ${CURDIR}/c8y-configuration-plugin-updated.toml    /etc/tedge/c8y/
    Execute Command    mv /etc/tedge/c8y/c8y-configuration-plugin-updated.toml /etc/tedge/c8y/c8y-configuration-plugin.toml
    Cumulocity.Should Support Configurations    ${DEFAULT_CONFIG}    /etc/tedge/tedge.toml    system.toml    CONFIG1    Config@2.0.0

*** Keywords ***

Test Setup
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y-configuration-plugin.toml    /etc/tedge/c8y/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config1.json         /etc/
    Execute Command    chown root:root /etc/tedge/c8y/c8y-configuration-plugin.toml /etc/config1.json
    ThinEdgeIO.Service Health Status Should Be Up    c8y-configuration-plugin
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
