*** Settings ***
Resource        ../../resources/common.resource
Library         ThinEdgeIO

Suite Setup     Setup    connect=${False}

Test Tags       theme:cli    theme:configuration


*** Test Cases ***
Validate default
    ${share_dir}=    Execute Command    tedge config get share.path    strip=${True}
    ${diag_dir}=    Execute Command    tedge config get diag.plugin_paths    strip=${True}
    ${log_dir}=    Execute Command    tedge config get log.plugin_paths    strip=${True}
    Should Be Equal    ${share_dir}    /usr/share
    Should Contain    ${diag_dir}    /usr/share/tedge/diag-plugins
    Should Contain    ${log_dir}    /usr/share/tedge/log-plugins

Change share.path
    Execute Command    tedge config set share.path /foo/bar
    ${share_dir}=    Execute Command    tedge config get share.path    strip=${True}
    ${diag_dir}=    Execute Command    tedge config get diag.plugin_paths    strip=${True}
    ${log_dir}=    Execute Command    tedge config get log.plugin_paths    strip=${True}
    Should Be Equal    ${share_dir}    /foo/bar
    Should Contain    ${diag_dir}    /foo/bar/tedge/diag-plugins
    Should Contain    ${log_dir}    /foo/bar/tedge/log-plugins
