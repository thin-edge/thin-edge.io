*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:software    theme:plugins


*** Test Cases ***
Install package with Epoch in version #2666
    ${FILE_URL}=    Cumulocity.Create Inventory Binary
    ...    package-with-epoch
    ...    package
    ...    file=${CURDIR}/package-with-epoch_1.2.3_all.deb
    ${OPERATION}=    Install Software
    ...    {"name": "package-with-epoch", "version": "2:1.2.3", "softwareType": "apt", "url": "${FILE_URL}"}
    ${OPERATION}=    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software
    ...    {"name": "package-with-epoch", "version": "2:1.2.3", "softwareType": "apt"}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}
