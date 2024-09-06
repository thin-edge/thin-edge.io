*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:software    theme:plugins


*** Test Cases ***
Install packages without overwriting config files
    Execute Command    tedge config set apt.dpk.options.config keepold
    # install package v1
    ${OPERATION}=    Install Software
    ...    {"name": "sampledeb", "version": "1.0.0", "softwareType": "apt", "url": "${FILE_URL_1}"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60

    # Modify the config file
    Execute Command    echo "Updated the config" | sudo tee -a /etc/sampledeb.cfg

    # install package v2
    ${OPERATION}=    Install Software
    ...    {"name": "sampledeb", "version": "1.0.0", "softwareType": "apt", "url": "${FILE_URL_1}"}
    ${OPERATION}=    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60

    # Assert to make sure installation of newer version did not update the config file
    ${output}=    Execute Command    cat /etc/sampledeb.cfg
    Should Contain    ${output}    conf 1.0
    Should Contain    ${output}    Updated the config

Install packages overwrite config files
    Execute Command    tedge config set apt.dpk.options.config keepnew
    # install package v1
    ${OPERATION}=    Install Software
    ...    {"name": "sampledeb", "version": "1.0.0", "softwareType": "apt", "url": "${FILE_URL_1}"}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60

    # Modify the config file
    Execute Command    echo "Updated the config" | sudo tee -a /etc/sampledeb.cfg

    # install package v2
    ${OPERATION}=    Install Software
    ...    {"name": "sampledeb", "version": "2.0.0", "softwareType": "apt", "url": "${FILE_URL_2}"}
    ${OPERATION}=    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60

    # Assert to make sure installation of newer version did not update the config file
    ${output}=    Execute Command    cat /etc/sampledeb.cfg
    Should Contain    ${output}    conf 2.0


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}

    ${FILE_URL_1}=    Cumulocity.Create Inventory Binary
    ...    sampledeb
    ...    package
    ...    file=${CURDIR}/sampledeb_1.0.0_all.deb
    Set Test Variable    $FILE_URL_1

    ${FILE_URL_2}=    Cumulocity.Create Inventory Binary
    ...    sampledeb
    ...    package
    ...    file=${CURDIR}/sampledeb_2.0.0_all.deb
    Set Test Variable    $FILE_URL_2
