*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:software    theme:plugins


*** Test Cases ***
Apply name filter
    ${packages_list}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt list --name tedge
    ...    exp_exit_code=0
    Should Match Regexp    ${packages_list}    ^tedge\\s+${VERSION}

Apply maintainer filter
    ${packages_list}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt list --maintainer thin-edge.*
    ...    exp_exit_code=0
    Should Match Regexp    ${packages_list}    c8y.*${VERSION}(.|\\n)*tedge\\s+${VERSION}

Apply both filters
    ${packages_list}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt list --name sudo --maintainer thin-edge.*
    ...    exp_exit_code=0
    Should Match Regexp    ${packages_list}    sudo(.|\\n)*tedge\\s+${VERSION}

No filters
    ${packages_list}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt list
    ...    exp_exit_code=0
    Should Match Regexp    ${packages_list}    sudo(.|\\n)*tedge\\s+${VERSION}

Both filters but name filter as empty string
    ${packages_list}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt list --name "" --maintainer thin-edge.*
    ...    exp_exit_code=0
    Should Match Regexp    ${packages_list}    c8y.*${VERSION}(.|\\n)*tedge\\s+${VERSION}

Both filters but maintainer filter as empty string
    ${packages_list}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt list --name tedge --maintainer ""
    ...    exp_exit_code=0
    Should Match Regexp    ${packages_list}    ^tedge\\s+${VERSION}

Both filters as empty string
    ${packages_list}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt list --name "" --maintainer ""
    ...    exp_exit_code=0
    Should Be Equal    ${packages_list}    ${EMPTY}


*** Keywords ***
Custom Setup
    Setup
    ${VERSION}=    Execute Command    tedge --version | cut -d' ' -f 2    strip=True
    Set Suite Variable    ${VERSION}
