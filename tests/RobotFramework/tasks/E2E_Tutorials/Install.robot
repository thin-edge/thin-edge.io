*** Settings ***

Resource              ../../../../resources/common.resource
Library               ThinEdgeIO    adapter=${ADAPTER}
Library               Cumulocity
Suite Setup            Setup    skip_bootstrap=True   
Suite Teardown        Get Logs


*** Variables ***

${ADAPTER}            ssh


*** Tasks ***

Install/Update of thinedge curl
   ${log}   Execute Command    curl -fsSL https://thin-edge.io/install.sh | sh -s
   Verify ThinEdgeIO is installed
   Uninstall ThinEdgeIO
   
Install/Update of thinedge wget
   ${log}    Execute Command    wget -O - https://thin-edge.io/install.sh | sh -s
   Verify ThinEdgeIO is installed
   Uninstall ThinEdgeIO

Update using a package manager
   ${log}    Execute Command    sudo apt-get update && yes | sudo apt-get install tedge-full
   Verify ThinEdgeIO is installed
   Uninstall ThinEdgeIO

Optional: Linux distributions without systemd curl
   ${OUTPUT}    Execute Command    curl -fsSL https://thin-edge.io/install-services.sh | sh -s    ignore_exit_code=True
   #Not verifiing this test step because the test running in container already exists: tests/RobotFramework/tests/installation/install_on_linux.robot
   #Checking only that the link is correct
   Should Contain    ${OUTPUT}    Welcome to the thin-edge.io community!

Optional: Linux distributions without systemd wget
   ${OUTPUT}    Execute Command    wget -O - https://thin-edge.io/install-services.sh | sh -s
   #Not verifiing this test step because the test running in container already exists: tests/RobotFramework/tests/installation/install_on_linux.robot
   #Checking only that the link is correct
   Should Contain    ${OUTPUT}    Welcome to the thin-edge.io community!

Manual repository setup and installation running with sudo
   ${OUTPUT}    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.deb.sh' | sudo bash
   Should Contain    ${OUTPUT}    The repository has been installed successfully - You're ready to rock!
   ${log}    Execute Command    sudo apt update
   Check repository creation
   ${log}    Execute Command    sudo apt-get install -y tedge-full
   Verify ThinEdgeIO is installed
   Uninstall ThinEdgeIO
   Remove created repository

Manual repository setup and installation running as root
   ${OUTPUT}    Execute Command    sudo su -c "whoami && curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.deb.sh' | bash && apt update && apt-get install -y tedge-full"
   Should Contain    ${OUTPUT}    root
   Should Contain    ${OUTPUT}    The repository has been installed successfully - You're ready to rock!
   Verify ThinEdgeIO is installed
   Uninstall ThinEdgeIO
   Remove created repository
     
Install via tarball
   ${log}    Execute Command    curl -fsSL https://thin-edge.io/install.sh | sh -s -- --package-manager tarball
   Verify ThinEdgeIO is installed
   Uninstall ThinEdgeIO


*** Keywords ***

Verify ThinEdgeIO is installed
   ${OUTPUT}    Execute Command    tedge --help
   Should Contain    ${OUTPUT}    tedge is the cli tool for thin-edge.io
   Log    ThinEdgeIO was successfully installed

Uninstall ThinEdgeIO
   Transfer To Device    ${CURDIR}/uninstall-thin-edge_io.sh    /var/local/share/uninstall-thin-edge_io.sh
   Execute Command    chmod a+x /var/local/share/uninstall-thin-edge_io.sh
   Execute Command    /var/local/share/uninstall-thin-edge_io.sh purge

#Verify ThinEdgeIO is uninstalled
   ${OUTPUT}    Execute Command    command -V tedge    exp_exit_code=!0 
   ${OUTPUT}    Execute Command    command -V tedge    exp_exit_code=!0

Check repository creation
   ${OUTPUT}    Execute Command    ls /etc/apt/sources.list.d/
   Should Contain    ${OUTPUT}    *.list
   ${OUTPUT}    Execute Command    apt-cache search tedge
   Should Contain    ${OUTPUT}    tedge - CLI tool use to control and configure thin-edge.io
   Should Contain    ${OUTPUT}    tedge - CLI tool use to control and configure thin-edge.io
   Should Contain    ${OUTPUT}    tedge-agent - thin-edge.io interacts with a Cloud Mapper and one or more Software Plugins
   Should Contain    ${OUTPUT}    tedge-apt-plugin - thin-edge.io plugin for software management using apt
   Should Contain    ${OUTPUT}    tedge-full - thin-edge.io virtual package to automatically install all tedge packages
   Should Contain    ${OUTPUT}    tedge-mapper - thin-edge.io mapper that translates thin-edge.io data model to c8y/az data model.
   Should Contain    ${OUTPUT}    tedge-watchdog - thin-edge.io component which checks the health of all the thin-edge.io components/services.
Remove created repository
   ${OUTPUT}    Execute Command    sudo rm /etc/apt/sources.list.d/thinedge-tedge-release.list
   Should Not Contain    ${OUTPUT}    thinedge-tedge-release.list
