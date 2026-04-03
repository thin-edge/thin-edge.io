*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:installation


*** Variables ***
${DEVICE_SN}        None
${CUSTOM_USER}      tedge-custom
${CUSTOM_GROUP}     tedge-custom


*** Test Cases ***
Install latest via script (from current branch)
    Execute Command    curl -fsSL https://thin-edge.io/install.sh | sh -s
    Tedge Version Should Match Regex    ^\\d+\\.\\d+\\.\\d+(-rc\\.\\d+)?$

    # Uninstall
    Uninstall tedge using local Script
    Tedge Should Not Be Installed

Install specific version via script (from current branch)
    [Documentation]    Remove the tedge.toml file as the software filter is not supported in older tedge versions
    ...    and unknown configuration causes problems
    # TODO: Remove reliance on the legacy get-thin-edge_io.sh script, however thin-edge.io/install.sh does not currently support installing a specific version
    Transfer To Device    ${CURDIR}/../../../../get-thin-edge_io.sh    /setup/
    Execute Command    [ -f /etc/tedge/tedge.toml ] && sed -i '/\\[software\\]/,/\\n/d' /etc/tedge/tedge.toml
    Execute Command    chmod a+x /setup/get-thin-edge_io.sh && sudo /setup/get-thin-edge_io.sh 0.8.1
    Tedge Version Should Be Equal    0.8.1

    # Uninstall
    Uninstall tedge using local Script
    Tedge Should Not Be Installed    tedge-agent

Install latest tedge via script
    Execute Command    curl -fsSL https://thin-edge.io/install.sh | sh -s
    Tedge Version Should Match Regex    ^\\d+\\.\\d+\\.\\d+(-rc\\.\\d+)?$

Install then uninstall latest tedge via script (from main branch)
    # Install (just install everything, don't set anything up)
    [Setup]    Setup    register=${False}
    Execute Command    dpkg -s tedge
    Execute Command    dpkg -s tedge-watchdog
    Execute Command    dpkg -s tedge-mapper
    Execute Command    dpkg -s tedge-agent
    Execute Command    dpkg -s c8y-firmware-plugin
    Execute Command    dpkg -s c8y-remote-access-plugin

    # Uninstall
    Execute Command
    ...    curl -sSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh | sudo sh -s purge
    Tedge Should Not Be Installed

Install tedge with custom user provided in system.toml
    [Documentation]    Verify that pre-seeding /etc/tedge/system.toml with a custom user/group
    ...    before installation forces tedge to use that user for all of its file ownership.

    Create Custom User And Group
    Install Tedge With System Config    custom_user_system.toml
    Setup Systemd Overrides For Custom User
    Verify Tedge Running With Custom User

Install tedge with empty user provided in system.toml
    [Documentation]    Verify that pre-seeding /etc/tedge/system.toml with a empty user/group
    ...    before installation forces tedge to use that user for all of its file ownership.

    Create Custom User And Group
    Install Tedge With System Config    empty_user_system.toml

    # Set the desired tedge user manually, instead of specifying it in system.toml
    Execute Command    tedge init --user ${CUSTOM_USER} --group ${CUSTOM_GROUP}

    Setup Systemd Overrides For Custom User
    Verify Tedge Running With Custom User


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}

Install tedge
    ${bootstrap_cmd}=    ThinEdgeIO.Get Bootstrap Command
    Execute Command    cmd=${bootstrap_cmd}

Connect device
    [Arguments]    ${external_id}
    Register Device With Cumulocity CA    external_id=${external_id}
    Execute Command    cmd=tedge connect c8y

Tedge Version Should Match Regex
    [Arguments]    ${expected}
    ${VERSION}=    Execute Command    tedge --version | cut -d' ' -f 2    strip=True
    Should Match Regexp    ${VERSION}    ${expected}

Tedge Version Should Be Equal
    [Arguments]    ${expected}
    ${VERSION}=    Execute Command    tedge --version | cut -d' ' -f 2    strip=True
    Should Be Equal    ${VERSION}    ${expected}

Uninstall tedge using local Script
    Transfer To Device    ${CURDIR}/../../../../uninstall-thin-edge_io.sh    /setup/
    Execute Command    chmod a+x /setup/uninstall-thin-edge_io.sh && sudo /setup/uninstall-thin-edge_io.sh purge

Tedge Should Not Be Installed
    [Arguments]    ${OPTIONAL_PACKAGES}=
    Execute Command    dpkg -s tedge    exp_exit_code=!0
    Execute Command    dpkg -s tedge-watchdog    exp_exit_code=!0
    Execute Command    dpkg -s tedge-mapper    exp_exit_code=!0
    Execute Command    dpkg -s tedge-agent    exp_exit_code=!0
    Execute Command    dpkg -s c8y-firmware-plugin    exp_exit_code=!0
    Execute Command    dpkg -s c8y-remote-access-plugin    exp_exit_code=!0

    IF    $OPTIONAL_PACKAGES
        Execute Command    dpkg -s ${OPTIONAL_PACKAGES}    exp_exit_code=!0
    END

Path Should Be Owned By
    [Documentation]    Assert that a directory exists and is owned by the expected user and group.
    [Arguments]    ${path}    ${user}    ${group}
    ${owner}=    Execute Command    stat -c '%U' ${path}    strip=True
    Should Be Equal    ${owner}    ${user}
    ${grp}=    Execute Command    stat -c '%G' ${path}    strip=True
    Should Be Equal    ${grp}    ${group}

Create Custom User And Group
    [Documentation]    Create a custom system user and group for running tedge services
    Execute Command    groupadd --system ${CUSTOM_GROUP}
    Execute Command    useradd --system --no-create-home --shell /sbin/nologin --gid ${CUSTOM_GROUP} ${CUSTOM_USER}

Install Tedge With System Config
    [Documentation]    Setup tedge system configuration by transferring the specified system.toml file,
    [Arguments]    ${system_toml_file}
    Execute Command    mkdir -p /etc/tedge
    Transfer To Device    ${CURDIR}/resources/${system_toml_file}    /etc/tedge/system.toml

    Install tedge

Setup Systemd Overrides For Custom User
    [Documentation]    Apply systemd overrides to tedge-agent and tedge-mapper-c8y services for custom user
    Execute Command    mkdir -p /etc/systemd/system/tedge-agent.service.d
    Transfer To Device    ${CURDIR}/resources/override.conf    /etc/systemd/system/tedge-agent.service.d/override.conf
    Execute Command    mkdir -p /etc/systemd/system/tedge-mapper-c8y.service.d
    Transfer To Device
    ...    ${CURDIR}/resources/override.conf
    ...    /etc/systemd/system/tedge-mapper-c8y.service.d/override.conf
    Execute Command    cmd=systemctl daemon-reload

    Execute Command    chown ${CUSTOM_USER}:${CUSTOM_GROUP} /setup/client.*

Verify Tedge Running With Custom User
    [Documentation]    Verify that tedge services are running and all relevant directories are owned by the custom user
    Connect device    ${DEVICE_SN}
    Service Should Be Running    tedge-agent
    Service Should Be Running    tedge-mapper-c8y

    FOR    ${path}    IN
    ...    /etc/tedge
    ...    /etc/tedge/.agent
    ...    /etc/tedge/.tedge-mapper-c8y
    ...    /etc/tedge/device
    ...    /etc/tedge/device-certs
    ...    /etc/tedge/operations
    ...    /etc/tedge/plugins
    ...    /etc/tedge/sm-plugins
    ...    /etc/tedge/mappers/c8y
    ...    /var/log/tedge/agent
    ...    /etc/tedge/system.toml
        Path Should Be Owned By    ${path}    ${CUSTOM_USER}    ${CUSTOM_GROUP}
    END
