*** Settings ***
Library             ThinEdgeIO
Library             String
Library             Collections

Suite Setup         Custom Suite Setup
Suite Teardown      Get Suite Logs
Test Teardown       Custom Test Teardown

Test Tags           theme:troubleshooting    theme:cli    theme:plugins


*** Test Cases ***
Run tedge diag collect
    Execute Command    tedge diag collect --tarball-name default    stdout=${False}
    File Should Exist    /tmp/default.tar.gz
    Execute Command    tar -xvzf /tmp/default.tar.gz -C /tmp
    Validate preset plugins    default

Run tedge diag collect with multiple plugin directories
    Transfer To Device    ${CURDIR}/00_template.sh    /setup/diag-plugins/00_template.sh
    Execute Command
    ...    tedge diag collect --tarball-name tedge-diag-now --plugin-dir /usr/share/tedge/diag-plugins --plugin-dir /setup/diag-plugins
    ...    stdout=${False}
    File Should Exist    /tmp/tedge-diag-now.tar.gz

    Execute Command    tar -xvzf /tmp/tedge-diag-now.tar.gz -C /tmp

    ${content}=    Execute Command    cat /tmp/tedge-diag-now/00_template/output.log
    Should Contain    ${content}    Output to stdout
    Should Contain    ${content}    Output to stderr

    ${content}=    Execute Command    cat /tmp/tedge-diag-now/00_template/template.log
    Should Contain    ${content}    Output to a file

    Validate preset plugins    tedge-diag-now

Exit with 0 when a plugin is non applicable
    Execute Command    tedge config set diag.plugin_paths /setup/diag-plugins
    Execute Command
    ...    printf '#!/bin/sh\nexit 2\n' > /setup/diag-plugins/98_not-applicable.sh && chmod +x /setup/diag-plugins/98_not-applicable.sh
    Execute Command    tedge diag collect --tarball-name tedge-diag-not-applicable    stdout=${False}

Exit with 1 when a plugin exits with non-zero"
    Execute Command    tedge config set diag.plugin_paths "/setup/diag-plugins","/usr/share/tedge/diag-plugins"
    Execute Command
    ...    printf '#!/bin/sh\nexit 1\n' > /setup/diag-plugins/99_error.sh && chmod +x /setup/diag-plugins/99_error.sh
    Execute Command
    ...    cmd=tedge diag collect --tarball-name tedge-diag-error    stdout=${False}
    ...    exp_exit_code=1

No tarball is created when there is no plugin
    Execute Command
    ...    tedge diag collect --tarball-name tedge-diag-no-plugin --plugin-dir /setup/diag-plugins    stdout=${False}
    ...    exp_exit_code=2
    File Should Not Exist    /tmp/tedge-diag-no-plugin.tar.gz


*** Keywords ***
Validate preset plugins
    [Arguments]    ${tarball_name}
    ${result}=    Execute Command    ls /usr/share/tedge/diag-plugins
    ${plugins}=    Split String    ${result}    \n
    ${cleared_list}=    Evaluate    [x for x in @{plugins} if x]
    FOR    ${plugin}    IN    @{cleared_list}
        ${base}=    Replace String    ${plugin}    .sh    ${EMPTY}
        File Should Exist    /tmp/${tarball_name}/${base}/output.log
    END

Custom Suite Setup
    Setup    skip_bootstrap=${True}
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect
    Execute Command    mkdir -p /setup/diag-plugins

Custom Test Teardown
    Execute Command    tedge config unset diag.plugin_paths
    Execute Command    rm -rf /setup/diag-plugins/*
