#Command to execute:    robot -d \results --timestampoutputs --log improve_tedge_apt_plugin_error_messages.html --report NONE --variable VERSION:0.7.4 --variable HOST:192.168.1.130 /thin-edge.io-fork/tests/RobotFramework/plugin_apt/improve_tedge_apt_plugin_error_messages.robot
*** Settings ***
Library    Browser
Library    OperatingSystem
Library    Dialogs
Library    SSHLibrary
Library    DateTime
Library    CryptoLibrary    variable_decryption=True
Suite Setup            Open Connection And Log In
Suite Teardown         SSHLibrary.Close All Connections

*** Variables ***
${HOST}           
${USERNAME}       pi
${PASSWORD}       crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=  
${VERSION}        0.*

*** Tasks ***
Uninstall tedge with purge
    Execute Command    wget https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh
    Execute Command    chmod a+x uninstall-thin-edge_io.sh
    Execute Command    ./uninstall-thin-edge_io.sh purge
Clear previous downloaded files if any
    Execute Command    rm *.deb | rm *.zip | rm *.sh*
Install the latest version thin-edge.io
    ${rc}=    Execute Command    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s ${VERSION}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
Download the rolldice Debian package
    ${rc}=    Execute Command    wget http://ports.ubuntu.com/pool/universe/r/rolldice/rolldice_1.16-1build1_arm64.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
Wrong package name
    Write    sudo /etc/tedge/sm-plugins/apt install thinml-3964 --file ./rolldice_1.16-1build1_arm64.deb
    Sleep    1s
    ${wpn}    Read
    Should Contain    ${wpn}    ERROR: Validation of ./rolldice_1.16-1build1_arm64.deb metadata failed, expected value for the Package is  rolldice, but provided  thinml-3964
Wrong version
    Write    sudo /etc/tedge/sm-plugins/apt install thinml-3964 --file ./rolldice_1.16-1build1_arm64.deb --module-version 1.0
    Sleep    1s
    ${wv}=    Read
    Should Contain    ${wv}    ERROR: Validation of ./rolldice_1.16-1build1_arm64.deb metadata failed, expected value for the Version is  1.16-1build1, but provided  1.0
Wrong type
    ${rc}=    Execute Command    echo "Not a debian package" >/tmp/foo.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    Write    sudo /etc/tedge/sm-plugins/apt install thinml-3964 --file /tmp/foo.deb
    Sleep    1s
    ${wv}=   Read
    Should Contain    ${wv}    ERROR: Parsing Debian package failed for `/tmp/foo.deb`, Error: dpkg-deb: error: '/tmp/foo.deb' is not a Debian format archive
    ${rc}=    Execute Command    rm /tmp/*.deb    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}

*** Keywords ***
Open Connection And Log In
   Open Connection     ${HOST}
   Login               ${USERNAME}        ${PASSWORD}
aarch64
    [Documentation]    Setting file name according architecture
    ${FILENAME}    Set Variable    debian-packages-aarch64-unknown-linux-gnu
    Log    ${FILENAME}
    Set Global Variable    ${FILENAME}
armv7
    [Documentation]    Setting file name according architecture
    ${FILENAME}    Set Variable    debian-packages-armv7-unknown-linux-gnueabihf
    Log    ${FILENAME}
    Set Global Variable    ${FILENAME}
