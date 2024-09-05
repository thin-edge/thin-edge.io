*** Settings ***
Documentation       Purpose of this test is to verify that the proper version number
...                 will be shown by using the tedge -V command.
...                 By executing the tedge -h command that Usage, Options and Commands
...                 will be shown

Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             String

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:cli


*** Variables ***
${version}      ${EMPTY}


*** Test Cases ***
Install thin-edge.io
    # running the script for installing latest version of tedge
    ${output}=    Execute Command
    ...    curl -fsSL https://thin-edge.io/install.sh | sh -s
    # Use apt-cache policy to get the installed version as the script lets apt handle this
    ${version}=    Execute Command
    ...    apt-cache policy tedge | grep "Installed:" | cut -d":" -f2 | sed 's/~rc\./-rc./' | xargs
    Set Suite Variable    ${version}

call tedge -V
    ${output}=    Execute Command    tedge -V
    # Check that the output of tedge -V returns the version which was installed
    Should Contain
    ...    ${output}
    ...    ${version}

call tedge -h
    ${output}=    Execute Command    tedge -h
    Should Contain    ${output}    Usage:
    Should Contain    ${output}    Options:
    Should Contain    ${output}    Commands:

call tedge help
    ${output}=    Execute Command    tedge help
    Should Contain    ${output}    Usage:
    Should Contain    ${output}    Options:
    Should Contain    ${output}    Commands:


*** Keywords ***
Custom Setup
    Setup    skip_bootstrap=True
    Execute Command    rm -f /etc/tedge/system.toml
