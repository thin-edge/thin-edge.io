*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:software    theme:plugins

*** Test Cases ***
Install packages without overwriting config files
    Execute Command    tedge config set apt.dpk.options.config keepold
    # install package v1
    Execute Command   /etc/tedge/sm-plugins/apt install sampledeb --file /setup/sampledeb_1.0.0_all.deb
    # Modify the config file
    Execute Command    echo "Updated the config" | sudo tee -a /etc/sampledeb.cfg

    # install package v2
    Execute Command   /etc/tedge/sm-plugins/apt install sampledeb --file /setup/sampledeb_2.0.0_all.deb
    # Assert to make sure installation of newer version did not update the config file
    ${output}=    Execute Command    cat /etc/sampledeb.cfg
    Should Contain    ${output}    conf 1.0
    Should Contain    ${output}    Updated the config

Install packages overwrite config files
    Execute Command    tedge config set apt.dpk.options.config keepnew
    # install package v1
    Execute Command   /etc/tedge/sm-plugins/apt install sampledeb --file /setup/sampledeb_1.0.0_all.deb
    # Modify the config file
    Execute Command    echo "Updated the config" | sudo tee -a /etc/sampledeb.cfg

    # install package v2
    Execute Command   /etc/tedge/sm-plugins/apt install sampledeb --file /setup/sampledeb_2.0.0_all.deb
    # Assert to make sure installation of newer version did not update the config file
    ${output}=    Execute Command    cat /etc/sampledeb.cfg
    Should Contain    ${output}    conf 2.0

*** Keywords ***
Custom Setup
    Setup
    ThinEdgeIO.Transfer To Device    ${CURDIR}/sampledeb_1.0.0_all.deb     /setup/sampledeb_1.0.0_all.deb
    ThinEdgeIO.Transfer To Device    ${CURDIR}/sampledeb_2.0.0_all.deb     /setup/sampledeb_2.0.0_all.deb

Custom Teardown
    Get Logs
    ThinEdgeIO.Remove Package Using APT    sampledeb
    Execute Command   rm /etc/sampledeb.cfg
