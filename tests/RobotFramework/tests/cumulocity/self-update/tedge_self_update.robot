*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:installation
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Update tedge version from previous using Cumulocity
    [Tags]    test:retry(1)    workaround
    ${PREV_VERSION}=    Set Variable    0.8.1
    # Install base version
    Execute Command    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s ${PREV_VERSION}

    # Disable service (as it was enabled by default in 0.8.1)
    Execute Command    systemctl stop tedge-mapper-az && systemctl disable tedge-mapper-az

    # Register device (using already installed version)
    Execute Command    cmd=test -f ./bootstrap.sh && env DEVICE_ID=${DEVICE_SN} ./bootstrap.sh --no-install --no-secure || true
    Device Should Exist                      ${DEVICE_SN}

    Restart Service    tedge-mapper-c8y    # WORKAROUND: #1731 Restart service to avoid suspected race condition causing software list message to be lost

    Device Should Have Installed Software    tedge,${PREV_VERSION}::apt    tedge_mapper,${PREV_VERSION}::apt    tedge_agent,${PREV_VERSION}::apt    tedge_watchdog,${PREV_VERSION}::apt    c8y_configuration_plugin,${PREV_VERSION}::apt    c8y_log_plugin,${PREV_VERSION}::apt    tedge_apt_plugin,${PREV_VERSION}::apt

    # Install desired version
    Create Local Repository
    [Documentation]    tedge-agent causes a problem where the operation is stuck in EXECUTING state
    ${OPERATION}=    Install Software    tedge,${NEW_VERSION}    tedge-mapper,${NEW_VERSION}    tedge-watchdog,${NEW_VERSION}    c8y-log-plugin,${NEW_VERSION}    c8y-configuration-plugin,${NEW_VERSION}    tedge-apt-plugin,${NEW_VERSION}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=180
    # Device Should Have Installed Software    tedge,${NEW_VERSION}::apt    tedge-mapper,${NEW_VERSION}::apt    tedge-agent,${NEW_VERSION}::apt    tedge-watchdog,${NEW_VERSION}::apt    c8y-configuration-plugin,${NEW_VERSION}::apt    c8y-log-plugin,${NEW_VERSION}::apt    tedge-apt-plugin,${NEW_VERSION}::apt
    Device Should Have Installed Software    tedge,${NEW_VERSION}::apt    tedge-mapper,${NEW_VERSION}::apt    tedge-watchdog,${NEW_VERSION}::apt    c8y-configuration-plugin,${NEW_VERSION}::apt    c8y-log-plugin,${NEW_VERSION}::apt    tedge-apt-plugin,${NEW_VERSION}::apt

    # Check if services are still stopped and disabled
    ${OUTPUT}    Execute Command    systemctl is-active tedge-mapper-az || exit 1    exp_exit_code=1    strip=True
    Should Be Equal    ${OUTPUT}    inactive    msg=Service should still be stopped
    ${OUTPUT}    Execute Command    systemctl is-enabled tedge-mapper-az || exit 1    exp_exit_code=1    strip=True
    Should Be Equal    ${OUTPUT}    disabled    msg=Service should still be disabled

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Suite Variable    $DEVICE_SN

    # Cleanup
    Execute Command    rm -f /etc/tedge/tedge.toml /etc/tedge/system.toml && sudo dpkg --configure -a && apt-get purge -y "tedge*" "c8y*"
    Execute Command    cmd=rm -f /etc/apt/sources.list.d/thinedge*.list /etc/apt/sources.list.d/tedge*.list    # Remove any existing repositories (due to candidate bug in <= 0.8.1)

Create Local Repository
    # Create local apt repo
    Execute Command    apt-get install -y --no-install-recommends dpkg-dev
    Execute Command    mkdir -p /opt/repository/local && find /setup -type f -name "*.deb" -exec cp {} /opt/repository/local \\;
    ${NEW_VERSION}=    Execute Command    find /setup -type f -name "tedge-mapper_*.deb" | sort -Vr | head -n1 | cut -d'_' -f 2    strip=True
    Set Suite Variable    $NEW_VERSION
    Execute Command    cd /opt/repository/local && dpkg-scanpackages -m . > Packages
    Execute Command    cmd=echo 'deb [trusted=yes] file:/opt/repository/local /' > /etc/apt/sources.list.d/tedge-local.list
