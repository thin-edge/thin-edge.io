*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:cli    theme:configuration


*** Test Cases ***
Check existence of init directories
    [Documentation]    During Custom Setup these folders were removed
    ...    this test step is confirming the deletion of the folders
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge/mosquitto-conf
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge/operations
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge/plugins
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge/device-certs
    ThinEdgeIO.Directory Should Not Exist    /var/tedge
    ThinEdgeIO.Directory Should Not Exist    /var/log/tedge

Tedge init and check creation of folders
    [Documentation]    Running tedge init should create these folders
    ...    this test step is confirming the creation of the folders
    Execute Command    sudo tedge init
    ThinEdgeIO.Directory Should Exist    /etc/tedge
    ThinEdgeIO.Directory Should Exist    /etc/tedge/mosquitto-conf
    ThinEdgeIO.Directory Should Exist    /etc/tedge/operations
    ThinEdgeIO.Directory Should Exist    /etc/tedge/plugins
    ThinEdgeIO.Directory Should Exist    /etc/tedge/device-certs
    ThinEdgeIO.Directory Should Exist    /var/tedge
    ThinEdgeIO.Directory Should Exist    /var/log/tedge

Check ownership of the folders
    [Documentation]    Running tedge init has created the folders assigning default user/group
    ...    this test step is confirming the default values for user/group
    Check Owner of Directory    /etc/tedge    tedge:tedge
    Check Owner of Directory    /etc/tedge/mosquitto-conf    tedge:tedge
    Check Owner of Directory    /etc/tedge/operations    tedge:tedge
    Check Owner of Directory    /etc/tedge/plugins    tedge:tedge
    Check Owner of Directory    /etc/tedge/device-certs    tedge:tedge
    Check Owner of Directory    /var/tedge    tedge:tedge
    Check Owner of Directory    /var/log/tedge    tedge:tedge

Change user/group and check the change
    [Documentation]    Running tedge init --user <user> --group <group>    is setting custom user/group
    ...    this test step is confirming the custom values for user/group
    Execute Command    sudo tedge init --user petertest --group petertest
    Check Owner of Directory    /etc/tedge    petertest:petertest
    Check Owner of Directory    /etc/tedge/mosquitto-conf    petertest:petertest
    Check Owner of Directory    /etc/tedge/operations    petertest:petertest
    Check Owner of Directory    /etc/tedge/plugins    petertest:petertest
    Check Owner of Directory    /etc/tedge/device-certs    petertest:petertest
    Check Owner of Directory    /var/tedge    petertest:petertest
    Check Owner of Directory    /var/log/tedge    petertest:petertest

Tedge init and check if default values are restored
    [Documentation]    Running tedge init after setting custom user/group should restore the default user/group
    ...    this test step is confirming the default values for user/group
    Execute Command    sudo tedge init
    Check Owner of Directory    /etc/tedge    tedge:tedge
    Check Owner of Directory    /etc/tedge/mosquitto-conf    tedge:tedge
    Check Owner of Directory    /etc/tedge/operations    tedge:tedge
    Check Owner of Directory    /etc/tedge/plugins    tedge:tedge
    Check Owner of Directory    /etc/tedge/device-certs    tedge:tedge
    Check Owner of Directory    /var/tedge    tedge:tedge
    Check Owner of Directory    /var/log/tedge    tedge:tedge

Tedge init sets user and group from system.toml
    Transfer To Device    ${CURDIR}/resources/custom_user_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    petertest:petertest
    [Teardown]    Restore Initial State

Tedge init defaults to tedge when user and group are absent in system.toml
    Transfer To Device    ${CURDIR}/resources/no_user_no_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    tedge:tedge
    [Teardown]    Restore Initial State

Tedge init leaves ownership unchanged when user and group are empty in system.toml
    Transfer To Device    ${CURDIR}/resources/empty_user_empty_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    root:root
    [Teardown]    Restore Initial State

Tedge init defaults both user and group to tedge when system.toml has an invalid user value
    Transfer To Device    ${CURDIR}/resources/invalid_user_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    tedge:tedge
    [Teardown]    Restore Initial State

Tedge init defaults both user and group to tedge when system.toml has an invalid group value
    Transfer To Device    ${CURDIR}/resources/custom_user_invalid_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    tedge:tedge
    [Teardown]    Restore Initial State

Tedge init CLI arguments override user and group values from system.toml
    Transfer To Device    ${CURDIR}/resources/custom_user_custom_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init --user petertest --group petertest
    Execute Command    sudo c8y-remote-access-plugin --init --user petertest --group petertest
    All Tedge Directories Should Be Owned By    petertest:petertest
    [Teardown]    Restore Initial State

Tedge init uses custom user and defaults group to tedge when user is custom and group is absent in system.toml
    Transfer To Device    ${CURDIR}/resources/custom_user_no_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    petertest:tedge
    [Teardown]    Restore Initial State

Tedge init defaults user to tedge and uses custom group when user is absent and group is custom in system.toml
    Transfer To Device    ${CURDIR}/resources/no_user_custom_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    tedge:petertest
    [Teardown]    Restore Initial State

Tedge init uses custom user and leaves group unchanged when user is custom and group is empty in system.toml
    Transfer To Device    ${CURDIR}/resources/custom_user_empty_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    petertest:root
    [Teardown]    Restore Initial State

Tedge init leaves user unchanged and uses custom group when user is empty and group is custom in system.toml
    Transfer To Device    ${CURDIR}/resources/empty_user_custom_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    root:petertest
    [Teardown]    Restore Initial State

Tedge init defaults user and leaves group unchanged when user is absent and group is empty in system.toml
    Transfer To Device    ${CURDIR}/resources/no_user_empty_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    tedge:root
    [Teardown]    Restore Initial State

Tedge init leaves user unchanged and group to tedge when user is empty and group not specified in system.toml
    Transfer To Device    ${CURDIR}/resources/empty_user_no_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Execute Command    sudo c8y-remote-access-plugin --init
    All Tedge Directories Should Be Owned By    root:tedge
    [Teardown]    Restore Initial State

Tedge cert download c8y creates private key owned by system.toml user/group
    [Documentation]    Check that the private key created during cert download is owned by
    ...    the user/group specified in system.toml, not the hardcoded "tedge" default.
    ...    This tests the case where tedge runs as root but files should be owned by a non-root user.
    [Tags]    theme:c8y
    Transfer To Device    ${CURDIR}/resources/custom_user_group_system.toml    /etc/tedge/system.toml
    Execute Command    sudo tedge init
    Set Cumulocity URLs
    ${credentials}=    Bulk Register Device With Cumulocity CA    ${DEVICE_SN}
    ${DOMAIN}=    Cumulocity.Get Domain
    Execute Command    sudo -u petertest tedge config set mqtt.bridge.built_in true
    Execute Command
    ...    sudo tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --url ${DOMAIN} --retry-every 5s --max-timeout 30s
    Path Should Have Permissions
    ...    path=/etc/tedge/device-certs/tedge-private-key.pem
    ...    owner_group=petertest:petertest
    [Teardown]    Restore Initial State

Run tedge as the current user
    [Documentation]    Check that users can also run commands using the current user
    ...    by explicitly opting into this behaviour by setting the user/group in the system.toml
    ...    to empty values. The test only starts processes manually and does not use an init system
    ...    so the init section of the system.toml file is configured to run no-ops on all init system
    ...    interactions called from various parts of thin-edge.io (e.g. tedge connect c8y)
    Transfer To Device    ${CURDIR}/resources/current_user_system.toml    /etc/tedge/system.toml

    # set the url values before so we don't have to handle the c8y.url vs. c8y.http/c8y.mqtt logic
    Set Cumulocity URLs

    # create initial permissions for a non-root user to
    Execute Command    sudo mkdir -p /etc/tedge /var/tedge /var/log/tedge
    Execute Command    sudo chown -R petertest:petertest /etc/tedge /var/tedge /var/log/tedge

    Execute Command    sudo -u petertest tedge init
    Execute Command    sudo -u petertest c8y-remote-access-plugin --init
    ${output}=    Execute Command
    ...    sudo -u petertest tedge config set sudo.enable false
    ...    stdout=${False}
    ...    stderr=${True}
    ...    strip=${True}
    Should Be Empty    ${output}    No warnings should be present
    Execute Command    sudo -u petertest tedge config set mqtt.bridge.built_in true

    # check that we can create/remove a self signed certificate
    Execute Command    sudo -u petertest tedge cert create --device-id ${DEVICE_SN}
    Execute Command    sudo -u petertest tedge cert remove

    # register device in Cumulocity
    ${credentials}=    Bulk Register Device With Cumulocity CA    ${DEVICE_SN}
    ${DOMAIN}=    Cumulocity.Get Domain
    Execute Command
    ...    sudo -u petertest tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --url ${DOMAIN} --retry-every 5s --max-timeout 30s
    Execute Command    sudo -u petertest tedge cert show c8y
    All Device Certificate Files Should Be Owned By    petertest:petertest

    # Run each service for a short amount of time as a smoke test
    Execute Command    sudo -u petertest timeout --preserve-status 5 tedge-agent    retries=1
    Execute Command    sudo -u petertest timeout --preserve-status 5 tedge-mapper c8y    retries=1
    Execute Command    sudo -u petertest timeout --preserve-status 5 c8y-firmware-plugin    retries=1

    # Run tedge-mapper c8y in the background, and check if the tedge connect works
    # Note: this assumes using the built-in bridge and that mosquitto is already running
    Execute Command    sudo -u petertest nohup tedge-mapper c8y &
    ${output}=    Execute Command    sudo -u petertest tedge connect c8y 2>&1
    Should Not Contain    ${output}    Failed to change ownership    msg=No ownership warnings/errors should be present

    # cert renewal
    Execute Command    sudo -u petertest tedge cert renew c8y 2>&1
    Execute Command    sudo -u petertest tedge reconnect c8y 2>&1
    All Device Certificate Files Should Be Owned By    petertest:petertest

    All Tedge Directories Should Be Owned By    petertest:petertest
    [Teardown]    Restore Initial State


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}
    Stop Service    tedge-agent
    Stop Service    c8y-firmware-plugin
    Stop Service    tedge-mapper-c8y
    Execute Command    sudo rm -rf /etc/tedge
    Execute Command    sudo rm -rf /var/tedge
    Execute Command    sudo rm -rf /var/log/tedge

Custom Teardown
    Get Suite Logs

Check Owner of Directory
    [Arguments]    ${directory_path}    ${expected_owner}
    Path Should Have Permissions    path=${directory_path}    owner_group=${expected_owner}

All Tedge Directories Should Be Owned By
    [Arguments]    ${expected_owner}
    FOR    ${dir}    IN    /etc/tedge    /etc/tedge/mosquitto-conf    /etc/tedge/operations
    ...    /etc/tedge/plugins    /etc/tedge/device-certs    /var/tedge    /var/log/tedge
    ...    /etc/tedge/operations/c8y/c8y_RemoteAccessConnect
        Check Owner of Directory    ${dir}    ${expected_owner}
    END

All Device Certificate Files Should Be Owned By
    [Arguments]    ${expected_owner}
    FOR    ${file}    IN
    ...    /etc/tedge/device-certs/tedge-certificate.pem
    ...    /etc/tedge/device-certs/tedge-private-key.pem
    ...    /etc/tedge/device-certs/tedge.csr
        Path Should Have Permissions    path=${file}    owner_group=${expected_owner}
    END

Restore Initial State
    Execute Command    rm -rf /etc/tedge
    Execute Command    rm -rf /var/tedge
    Execute Command    rm -rf /var/log/tedge
