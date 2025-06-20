*** Settings ***
Library             String
Library             ThinEdgeIO

Suite Setup         Custom Suite Setup
Suite Teardown      Get Suite Logs
Test Teardown       Custom Test Teardown

Test Tags           theme:troubleshooting    theme:cli    theme:plugins


*** Test Cases ***
Run tedge diag collect
    ${stdout}=    Execute Command    tedge diag collect --name default    strip=${True}
    File Should Exist    /tmp/default.tar.gz
    Directory Should Not Exist    /tmp/default
    Should Be Equal    ${stdout}    /tmp/default.tar.gz

    Execute Command    tar -xvzf /tmp/default.tar.gz -C /tmp
    Validate preset plugins    default

    File Should Exist    /tmp/default/summary.log
    ${summary}=    Execute Command    cat /tmp/default/summary.log
    Should Contain    ${summary}    Executing

Run tedge diag collect with multiple plugin directories
    Transfer To Device    ${CURDIR}/00_template.sh    /setup/diag-plugins/00_template.sh
    Execute Command    chown root:root /setup/diag-plugins/00_template.sh

    Execute Command
    ...    tedge diag collect --name tedge-diag-now --plugin-dir /usr/share/tedge/diag-plugins --plugin-dir /setup/diag-plugins
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
    Execute Command    tedge diag collect --name tedge-diag-not-applicable    stdout=${False}

Exit with 1 when a plugin exits with non-zero"
    Execute Command    tedge config set diag.plugin_paths "/setup/diag-plugins","/usr/share/tedge/diag-plugins"
    Execute Command
    ...    printf '#!/bin/sh\nexit 1\n' > /setup/diag-plugins/99_error.sh && chmod +x /setup/diag-plugins/99_error.sh
    Execute Command
    ...    cmd=tedge diag collect --name tedge-diag-error    stdout=${False}
    ...    exp_exit_code=1

No tarball is created when there is no plugin
    Execute Command
    ...    tedge diag collect --name tedge-diag-no-plugin --plugin-dir /setup/diag-plugins    stdout=${False}
    ...    exp_exit_code=2
    File Should Not Exist    /tmp/tedge-diag-no-plugin.tar.gz

Invalid plugin is skipped
    Execute Command    touch /setup/diag-plugins/not_plugin
    ${stderr}=    Execute Command
    ...    tedge diag collect --name tedge-diag-invalid --plugin-dir /setup/diag-plugins --plugin-dir /usr/share/tedge/diag-plugins --keep-dir
    ...    stdout=${False}
    ...    stderr=${True}
    ${summary}=    Execute Command    cat /tmp/tedge-diag-invalid/summary.log
    Should Contain    ${stderr}    (not executable)
    Should Contain    ${summary}    (not executable)

Extra security guard when the command is run by root
    Execute Command
    ...    touch /setup/diag-plugins/owned_by_non_root.sh && chown tedge:tedge /setup/diag-plugins/owned_by_non_root.sh && chmod +x /setup/diag-plugins/owned_by_non_root.sh
    Execute Command
    ...    sudo touch /setup/diag-plugins/writable_by_other_users.sh && chmod a+wx /setup/diag-plugins/writable_by_other_users.sh
    ${stderr}=    Execute Command
    ...    sudo tedge diag collect --plugin-dir /setup/diag-plugins
    ...    stdout=${False}
    ...    stderr=${True}
    ...    ignore_exit_code=${True}
    Should Contain    ${stderr}    owned_by_non_root.sh (not owned by root)
    Should Contain    ${stderr}    writable_by_other_users.sh (writable by non-root users)


*** Keywords ***
Validate preset plugins
    [Arguments]    ${tarball_name}
    ${result}=    Execute Command    ls /usr/share/tedge/diag-plugins
    ${plugins}=    Split String    ${result}    \n
    ${filtered_plugins}=    Evaluate    [x for x in ${plugins} if x and not x.endswith('.ignore')]
    FOR    ${plugin}    IN    @{filtered_plugins}
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
