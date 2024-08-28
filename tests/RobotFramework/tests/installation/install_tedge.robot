*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:installation
Test Setup       Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Install latest via script (from current branch)
    Execute Command    curl -fsSL https://thin-edge.io/install.sh | sh -s
    Tedge Version Should Match Regex    ^\\d+\\.\\d+\\.\\d+(-rc\\.\\d+)?$

    # Uninstall
    Uninstall tedge using local Script
    Tedge Should Not Be Installed

Install specific version via script (from current branch)
    [Documentation]    Remove the tedge.toml file as the software filter is not supported in older tedge versions
    ...                and unknown configuration causes problems
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
    Execute Command    ./bootstrap.sh --install --no-bootstrap --no-connect
    Execute Command    dpkg -s tedge
    Execute Command    dpkg -s tedge-watchdog
    Execute Command    dpkg -s tedge-mapper
    Execute Command    dpkg -s tedge-agent
    Execute Command    dpkg -s c8y-firmware-plugin
    Execute Command    dpkg -s c8y-remote-access-plugin

    # Uninstall
    Execute Command    curl -sSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh | sudo sh -s purge
    Tedge Should Not Be Installed

*** Keywords ***

Custom Setup
    Setup    skip_bootstrap=True

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
