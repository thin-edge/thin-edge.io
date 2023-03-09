*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:installation
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Variables ***
${APT_INSTALL}    apt-get install -y \
    ...    tedge \
    ...    tedge-mapper \
    ...    tedge-agent \
    ...    tedge-apt-plugin \
    ...    c8y-configuration-plugin \
    ...    c8y-log-plugin \
    ...    c8y-firmware-plugin \
    ...    tedge-watchdog

*** Test Cases ***

Install thin-edge via apt
    Execute Command    apt-get update
    Execute Command    ${APT_INSTALL}

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Suite Variable    $DEVICE_SN

    # Cleanup
    Execute Command    rm -rf /etc/tedge && sudo dpkg --configure -a && apt-get purge -y "tedge*" "c8y*"
    Execute Command    cmd=rm -f /etc/apt/sources.list.d/thinedge*.list /etc/apt/sources.list.d/tedge*.list    # Remove any existing repositories (due to candidate bug in <= 0.8.1)

    # Create local apt repo
    Create Local Repository

Create Local Repository
    Execute Command    apt-get install -y --no-install-recommends dpkg-dev
    Execute Command    mkdir -p /opt/repository/local && find /setup -type f -name "*.deb" -exec cp {} /opt/repository/local \\;
    ${NEW_VERSION}=    Execute Command    find /setup -type f -name "tedge-mapper_*.deb" | sort -Vr | head -n1 | cut -d'_' -f 2    strip=True
    Set Suite Variable    $NEW_VERSION
    Execute Command    cd /opt/repository/local && dpkg-scanpackages -m . > Packages
    Execute Command    cmd=echo 'deb [trusted=yes] file:/opt/repository/local /' > /etc/apt/sources.list.d/tedge-local.list
