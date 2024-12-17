*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:installation


*** Test Cases ***
Update tedge version from previous using Cumulocity
    [Tags]    test:retry(1)    workaround

    ${PREV_VERSION}=    Set Variable    0.8.1
    # Install base version
    Execute Command
    ...    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s ${PREV_VERSION}

    # Disable service (as it was enabled by default in 0.8.1)
    Execute Command    systemctl stop tedge-mapper-az && systemctl disable tedge-mapper-az

    # Register device (using already installed version)
    Execute Command
    ...    cmd=test -f ./bootstrap.sh && env DEVICE_ID=${DEVICE_SN} ./bootstrap.sh --no-install --no-secure || true
    Device Should Exist    ${DEVICE_SN}

    # WORKAROUND: #1731 Restart service to avoid suspected race condition causing software list message to be lost
    Restart Service
    ...    tedge-mapper-c8y

    # Note: Software type is reported as a part of version in thin-edge 0.8.1
    Device Should Have Installed Software
    ...    tedge,${PREV_VERSION}::apt
    ...    tedge_mapper,${PREV_VERSION}::apt
    ...    tedge_agent,${PREV_VERSION}::apt
    ...    tedge_watchdog,${PREV_VERSION}::apt
    ...    c8y_configuration_plugin,${PREV_VERSION}::apt
    ...    c8y_log_plugin,${PREV_VERSION}::apt
    ...    tedge_apt_plugin,${PREV_VERSION}::apt

    # Install desired version
    Create Local Repository
    ${operation}=    Install Software
    ...    tedge,${NEW_VERSION}
    ...    tedge-mapper,${NEW_VERSION}
    ...    tedge-agent,${NEW_VERSION}
    ...    tedge-watchdog,${NEW_VERSION}
    ...    tedge-apt-plugin,${NEW_VERSION}
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180

    # Software list reported by the former agent, which is still running
    # but formatted with by the c8y-mapper, which has just been installed
    Device Should Have Installed Software
    ...    {"name": "tedge", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-mapper", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-agent", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-watchdog", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-apt-plugin", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}

    # Restart tedge-agent from Cumulocity
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180

    # Software list reported by the new agent
    Device Should Have Installed Software
    ...    {"name": "tedge", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-mapper", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-agent", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-watchdog", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-apt-plugin", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}

    # Check if services are still stopped and disabled
    ${OUTPUT}=    Execute Command    systemctl is-active tedge-mapper-az || exit 1    exp_exit_code=1    strip=True
    Should Be Equal    ${OUTPUT}    inactive    msg=Service should still be stopped
    ${OUTPUT}=    Execute Command    systemctl is-enabled tedge-mapper-az || exit 1    exp_exit_code=1    strip=True
    Should Be Equal    ${OUTPUT}    disabled    msg=Service should still be disabled

    # Check that the mapper is reacting to operations after the upgrade
    # Notes:
    # * Bug as seen in the past: https://github.com/thin-edge/thin-edge.io/issues/2545
    # * PR to switch to using devicecontrol c8y topic: https://github.com/thin-edge/thin-edge.io/issues/1718
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin
    Operation Should Be SUCCESSFUL    ${operation}

Refreshes mosquitto bridge configuration
    ${PREV_VERSION}=    Set Variable    0.10.0
    # Install base version
    Execute Command
    ...    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s ${PREV_VERSION}

    # Register device (using already installed version)
    Execute Command
    ...    cmd=test -f ./bootstrap.sh && env DEVICE_ID=${DEVICE_SN} ./bootstrap.sh --no-install --no-secure || true
    Device Should Exist    ${DEVICE_SN}

    # get bridge modification time
    ${before_upgrade_time}=    Execute Command    stat /etc/tedge/mosquitto-conf/c8y-bridge.conf -c %Y    strip=True

    # Install newer version
    Create Local Repository
    ${OPERATION}=    Install Software
    ...    tedge,${NEW_VERSION}
    ...    tedge-mapper,${NEW_VERSION}
    ...    tedge-agent,${NEW_VERSION}
    ...    tedge-watchdog,${NEW_VERSION}
    ...    tedge-apt-plugin,${NEW_VERSION}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=180

    # TODO: check that this new configuration is actually used by mosquitto
    ${c8y_bridge_mod_time}=    Execute Command    stat /etc/tedge/mosquitto-conf/c8y-bridge.conf -c %Y    strip=True
    Should Not Be Equal    ${c8y_bridge_mod_time}    ${before_upgrade_time}

    # Mosquitto should be restarted with new bridge
    Execute Command
    ...    cmd=sh -c '[ $(journalctl -u mosquitto | grep -c "Loading config file /etc/tedge/mosquitto-conf/c8y-bridge.conf") = 2 ]'

Update tedge version from base to current using Cumulocity
    # Install base version (the latest official release) with self-update capability
    Execute Command    wget -O - thin-edge.io/install.sh | sh -s
    Execute Command    cd /setup && test -f ./bootstrap.sh && ./bootstrap.sh --no-install --no-secure
    Device Should Exist    ${DEVICE_SN}
    ${pid_before}=    Service Should Be Running    tedge-agent

    # Upgrade to current version
    Create Local Repository
    ${OPERATION}=    Install Software
    ...    tedge,${NEW_VERSION}
    ...    tedge-mapper,${NEW_VERSION}
    ...    tedge-agent,${NEW_VERSION}
    ...    tedge-watchdog,${NEW_VERSION}
    ...    tedge-apt-plugin,${NEW_VERSION}

    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=300
    Device Should Have Installed Software
    ...    {"name": "tedge", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-mapper", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-agent", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-watchdog", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-apt-plugin", "softwareType": "apt", "version": "${NEW_VERSION_ESCAPED}"}

    ${pid_after}=    Service Should Be Running    tedge-agent
    Should Not Be Equal    ${pid_before}    ${pid_after}

Update tedge Using a Custom Software Update Workflow
    [Documentation]    thin-edge.io needs to support updating across a non-versioned workflow
    ...    which occurs when users are updating from tedge <= 1.3.1 to tedge >= 1.4.0
    ...    Once the new version of tedge-agent starts, it needs to recognize workflows
    ...    that don't have a version (as the workflow version feature did not exist prior to tedge 1.4.0)
    ${PREV_VERSION}=    Set Variable    1.3.1

    # Install base version (using apt package pinning, then remove it)
    Pin thin-edge.io APT Packages    ${PREV_VERSION}
    Execute Command    cmd=curl -fsSL thin-edge.io/install.sh | sh -s
    Unpin thin-edge.io APT Packages

    # Register device (using already installed version)
    Execute Command
    ...    cmd=test -f ./bootstrap.sh && env DEVICE_ID=${DEVICE_SN} ./bootstrap.sh --no-install --no-secure || true
    Device Should Exist    ${DEVICE_SN}

    Device Should Have Installed Software
    ...    tedge-agent,${PREV_VERSION}

    ${agent_version}=    Execute Command    tedge-agent --version    strip=${True}
    Should Be Equal As Strings    ${agent_version}    tedge-agent ${PREV_VERSION}

    # Allow tedge user to restart services (used in the workflow)
    Execute Command
    ...    cmd=echo "tedge ALL = (ALL) NOPASSWD: /usr/bin/systemctl restart *" | sudo tee /etc/sudoers.d/tedge_admin
    # Copy Workflow
    Transfer To Device    ${CURDIR}/software_update.toml    /etc/tedge/operations/
    # Reload the workflows (as tedge <= 1.3.1 did not support reloading workflows at runtime)
    Restart Service    tedge-agent

    # Configure local repository containing the new version
    Create Local Repository

    # Note: this just trigger the operation, the contents does not
    # actually matter as the workflow uses some hardcoded values
    ${operation}=    Install Software
    ...    tedge-agent,${NEW_VERSION}
    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    ${agent_version}=    Execute Command    tedge-agent --version    strip=${True}
    Should Be Equal As Strings    ${agent_version}    tedge-agent ${NEW_VERSION}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Suite Variable    $DEVICE_SN

    # Cleanup
    Execute Command
    ...    rm -f /etc/tedge/tedge.toml /etc/tedge/system.toml && sudo dpkg --configure -a && apt-get purge -y "tedge*" "c8y*"
    # Remove any existing repositories (due to candidate bug in <= 0.8.1)
    Execute Command
    ...    cmd=rm -f /etc/apt/sources.list.d/thinedge*.list /etc/apt/sources.list.d/tedge*.list

Create Local Repository
    [Arguments]    ${packages_dir}=/setup/packages
    # Create local apt repo
    Execute Command    apt-get update && apt-get install -y --no-install-recommends dpkg-dev
    Execute Command
    ...    mkdir -p /opt/repository/local && find ${packages_dir} -type f -name "*.deb" -exec cp {} /opt/repository/local \\;
    ${NEW_VERSION}=    Execute Command
    ...    find ${packages_dir} -type f -name "tedge-mapper_*.deb" | sort -Vr | head -n1 | cut -d'_' -f 2
    ...    strip=True
    Set Suite Variable    $NEW_VERSION
    ${NEW_VERSION_ESCAPED}=    Escape Pattern    ${NEW_VERSION}    is_json=${True}
    Set Suite Variable    $NEW_VERSION_ESCAPED
    Execute Command    cd /opt/repository/local && dpkg-scanpackages -m . > Packages
    Execute Command
    ...    cmd=echo 'deb [trusted=yes] file:/opt/repository/local /' > /etc/apt/sources.list.d/tedge-local.list

Pin thin-edge.io APT Packages
    [Arguments]    ${VERSION}
    Transfer To Device    ${CURDIR}/apt_package_pinning    /etc/apt/preferences.d/tedge
    Execute Command    cmd=sed -i 's/%%VERSION%%/${VERSION}/g' /etc/apt/preferences.d/tedge

Unpin thin-edge.io APT Packages
    Execute Command    cmd=rm -f /etc/apt/preferences.d/tedge
    # Remove thin-edge.io public repositories to avoid affecting the selected version
    Execute Command    cmd=rm -f /etc/apt/sources.list.d/thinedge-*.list
