*** Settings ***
Resource              ../../../../resources/common.resource
Library               SSHLibrary
# Library               ThinEdgeIO
Library               Cumulocity
Library               Process
Test Setup           Setup    
# Suite Teardown         Get Logs
Suite Teardown        Close Connection
    
*** Variables ***
${REMOTE_FILE}    output.txt
${LOCAL_FILE}    output1.txt

*** Tasks ***

Install/Update of thinedge curl
   ${log}   Execute Command    curl -fsSL https://thin-edge.io/install.sh | sh -s
   

Install/Update of thinedge wget
   ${log}    Execute Command    wget -O - https://thin-edge.io/install.sh | sh -s

Update using a package manager
   ${log}    Execute Command    sudo apt-get update && yes | sudo apt-get install tedge-full

Optional: Linux distributions without systemd curl
   ${log}    Execute Command    curl -fsSL https://thin-edge.io/install-services.sh | sh -s

Optional: Linux distributions without systemd wget
   ${log}    Execute Command    wget -O - https://thin-edge.io/install-services.sh | sh -s

Manual repository setup and installation running with sudo
   ${log}    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.deb.sh' | sudo bash

Manual repository setup and installation running as root
   ${log}    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-release/setup.deb.sh' | bash
     
Install via tarball
   ${log}    Execute Command    curl -fsSL https://thin-edge.io/install.sh | sh -s -- --package-manager tarball


*** Keywords ***

Setup
   Open Connection    ${SSH_CONFIG}[hostname]
   Login    ${SSH_CONFIG}[username]    ${SSH_CONFIG}[password]
