*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:installation
Test Setup       Custom Setup
Test Teardown    Get Logs

*** Test Cases ***
Install latest via script (from current branch)
    Transfer To Device    ${CURDIR}/../../../../get-thin-edge_io.sh    /setup/
    Execute Command    chmod a+x /setup/get-thin-edge_io.sh && sudo /setup/get-thin-edge_io.sh
    Tedge Version Should Match Regex    ^\\d+\\.\\d+\\.\\d+$

Install specific version via script (from current branch)
    Transfer To Device    ${CURDIR}/../../../../get-thin-edge_io.sh    /setup/
    Execute Command    chmod a+x /setup/get-thin-edge_io.sh && sudo /setup/get-thin-edge_io.sh 0.8.1
    Tedge Version Should Be Equal    0.8.1

Install latest tedge via script (from main branch)
    Execute Command    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
    Tedge Version Should Match Regex    ^\\d+\\.\\d+\\.\\d+$

Install then uninstall latest tedge via script (from main branch)
    # Install (just install everything, don't set anything up)
    Execute Command    ./bootstrap.sh --install --no-bootstrap --no-connect
    Execute Command    dpkg -s tedge
    Execute Command    dpkg -s tedge-mapper
    Execute Command    dpkg -s tedge-agent
    Execute Command    dpkg -s c8y-log-plugin
    Execute Command    dpkg -s c8y-configuration-plugin
    Execute Command    dpkg -s c8y-firmware-plugin
    Execute Command    dpkg -s c8y-remote-access-plugin

    # Uninstall
    Execute Command    curl -sSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh | sudo sh -s purge
    Execute Command    dpkg -s tedge    exp_exit_code=!0
    Execute Command    dpkg -s tedge-mapper    exp_exit_code=!0
    Execute Command    dpkg -s tedge-agent    exp_exit_code=!0
    Execute Command    dpkg -s c8y-log-plugin    exp_exit_code=!0
    Execute Command    dpkg -s c8y-configuration-plugin    exp_exit_code=!0
    Execute Command    dpkg -s c8y-firmware-plugin    exp_exit_code=!0
    Execute Command    dpkg -s c8y-remote-access-plugin    exp_exit_code=!0

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
