*** Settings ***
Library             ThinEdgeIO

Suite Setup         Custom Suite Setup
Suite Teardown      Get Suite Logs
Test Teardown       Custom Test Teardown

Test Tags           theme:troubleshooting    theme:cli    theme:plugins


*** Test Cases ***
Run tedge diag collect
    Transfer To Device    ${CURDIR}/00_template.sh    /etc/tedge/diag-plugins/00_template.sh
    Execute Command    tedge diag collect --tarball-name tedge-diag-now
    File Should Exist    /tmp/tedge-diag-now.tar.gz

    Execute Command    tar -xvzf /tmp/tedge-diag-now.tar.gz -C /tmp

    ${content}=    Execute Command    cat /tmp/tedge-diag-now/00_template/output.log
    Should Contain    ${content}    Output to stdout
    Should Contain    ${content}    Output to stderr

    ${content}=    Execute Command    cat /tmp/tedge-diag-now/00_template/template.log
    Should Contain    ${content}    Output to a file

Exit with 0 when a plugin is non applicable
    Execute Command
    ...    printf '#!/bin/sh\nexit 2\n' > /etc/tedge/diag-plugins/98_not-applicable.sh && chmod +x /etc/tedge/diag-plugins/98_not-applicable.sh
    Execute Command    tedge diag collect --tarball-name tedge-diag-not-applicable

Exit with 1 when a plugin exits with non-zero
    Execute Command
    ...    printf '#!/bin/sh\nexit 1\n' > /etc/tedge/diag-plugins/99_error.sh && chmod +x /etc/tedge/diag-plugins/99_error.sh
    Execute Command    tedge diag collect --tarball-name tedge-diag-error    exp_exit_code=1

No tarball is created when there is no plugin
    Execute Command    tedge diag collect --tarball-name tedge-diag-no-plugin    exp_exit_code=2
    File Should Not Exist    /tmp/tedge-diag-no-plugin.tar.gz


*** Keywords ***
Custom Suite Setup
    Setup    skip_bootstrap=${True}
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect

Custom Test Teardown
    Execute Command    rm -rf /etc/tedge/diag-plugins/*
