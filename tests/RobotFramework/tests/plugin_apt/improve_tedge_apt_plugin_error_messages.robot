*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs
Test Timeout        5 minutes

Test Tags           theme:software    theme:plugins


*** Test Cases ***
Wrong package name
    ${wpn}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt install thinml-3964 --file "${DEB_FILE}"
    ...    exp_exit_code=5
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain
    ...    ${wpn}
    ...    ERROR: Validation of ${DEB_FILE} metadata failed, expected value for the Package is
    ...    rolldice, but provided
    ...    thinml-3964

Wrong version
    ${wv}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt install thinml-3964 --file "${DEB_FILE}" --module-version 1.0
    ...    exp_exit_code=5
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain
    ...    ${wv}
    ...    ERROR: Validation of ${DEB_FILE} metadata failed, expected value for the Version is
    ...    1.16-1build1, but provided
    ...    1.0

Wrong type
    Execute Command    echo "Not a debian package" >/tmp/foo.deb
    ${wv}=    Execute Command
    ...    sudo /etc/tedge/sm-plugins/apt install thinml-3964 --file /tmp/foo.deb
    ...    exp_exit_code=5
    ...    stdout=${False}
    ...    stderr=${True}
    Should Contain
    ...    ${wv}
    ...    ERROR: Parsing Debian package failed for `/tmp/foo.deb`, Error: dpkg-deb: error: '/tmp/foo.deb' is not a Debian format archive
    Execute Command    rm /tmp/*.deb


*** Keywords ***
Custom Setup
    Setup
    Execute Command    apt-get update && apt-get install --download-only rolldice
    ${DEB_FILE}=    Execute Command    find /var/cache/apt/archives -type f -name "rolldice_*.deb"    strip=True
    Set Suite Variable    ${DEB_FILE}
