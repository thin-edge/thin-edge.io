*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Teardown      Custom Teardown
Test Setup          Custom Setup

Test Tags           theme:cli    theme:configuration


*** Test Cases ***
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
    ${output}=    Execute Command    sudo -u petertest tedge reconnect c8y 2>&1
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
