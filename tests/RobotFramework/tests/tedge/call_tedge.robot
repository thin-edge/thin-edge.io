*** Settings ***
Documentation    Purpose of this test is to verify that the proper version number
...              will be shown by using the tedge -V command.
...              By executing the tedge -h command that USAGE, OPTIONS and SUBCOMMANDS
...              will be shown
...              By executing the tedge -h -V command combination of both previous
...              commands will be shown

Resource    ../../resources/common.resource
Library    ThinEdgeIO
Library    String

Test Tags    theme:cli
Suite Setup            Custom Setup
Suite Teardown         Get Logs

*** Variables ***
${version}

*** Test Cases ***
Install thin-edge.io
    ${output}=    Execute Command    curl -fsSL https://thin-edge.io/install.sh | sh -s    #running the script for installing latest version of tedge
    # Use apt-cache policy to get the installed version as the script lets apt handle this
    ${version}=    Execute Command    apt-cache policy tedge | grep "Installed:" | cut -d":" -f2 | xargs
    Set Suite Variable    ${version}

call tedge -V
    ${output}=    Execute Command    tedge -V
    Should Contain    ${output}    ${version}    # Check that the output of tedge -V returns the version which was installed

call tedge -h
    ${output}=    Execute Command    tedge -h
    Should Contain    ${output}    USAGE:
    Should Contain    ${output}    OPTIONS:
    Should Contain    ${output}    SUBCOMMANDS:

call tedge -h -V
    ${output}=    Execute Command    tedge -h -V   # Execute command to call help and check the version at same time
    Should Contain    ${output}    ${version}    # Check that the output of tedge -V returns the version which was installed
    Should Contain    ${output}    USAGE:
    Should Contain    ${output}    OPTIONS:
    Should Contain    ${output}    SUBCOMMANDS:

call tedge help
    ${output}=    Execute Command    tedge help
    Should Contain    ${output}    USAGE:
    Should Contain    ${output}    OPTIONS:
    Should Contain    ${output}    SUBCOMMANDS:

*** Keywords ***

Custom Setup
    Setup    skip_bootstrap=True
    Execute Command    rm -f /etc/tedge/system.toml
