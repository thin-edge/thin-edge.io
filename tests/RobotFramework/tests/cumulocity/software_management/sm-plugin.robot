*** Settings ***

Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             Collections

Test Setup         Custom Setup
Test Teardown      Custom Teardown

Test Tags           theme:software

*** Test Cases ***

Check max_packages default value
    [Documentation]    Don't put an explicit max value to make the test more flexible against future tweaks to the default value.
    ...                The main point is to prevent accidentally using a small default value which is likely to truncate the packages unexpectedly
    Execute Command    sudo tedge config unset software.plugin.max_packages
    ${default_value}=    Execute Command    sudo tedge config get software.plugin.max_packages
    Should Be True    int(${default_value}) > 100

Limit number of packages
    Execute Command    sudo tedge config set software.plugin.max_packages 5
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software
    ...    {"name": "dummy1-0001", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0002", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0003", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0004", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0005", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy2-0001", "version": "1.0.0", "softwareType": "dummy2"}
    ...    {"name": "dummy2-0002", "version": "1.0.0", "softwareType": "dummy2"}
    ...    {"name": "dummy2-0003", "version": "1.0.0", "softwareType": "dummy2"}
    ...    {"name": "dummy2-0004", "version": "1.0.0", "softwareType": "dummy2"}
    ...    {"name": "dummy2-0005", "version": "1.0.0", "softwareType": "dummy2"}
    Length Should Be    ${software}    10

Limit number of packages to 1
    Execute Command    sudo tedge config set software.plugin.max_packages 1
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software
    ...    {"name": "dummy1-0001", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy2-0001", "version": "1.0.0", "softwareType": "dummy2"}
    Length Should Be    ${software}    2

Don't limit number of packages
    Execute Command    sudo tedge config set software.plugin.max_packages 0
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software
    ...    {"name": "dummy1-0001", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy2-0001", "version": "1.0.0", "softwareType": "dummy2"}
    Length Should Be    ${software}    3000

sm-plugins should work without sudo installed and running as root
    Execute Command    sudo tedge config set software.plugin.max_packages 1
    Set Service User    tedge-agent    root
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software
    ...    {"name": "dummy1-0001", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy2-0001", "version": "1.0.0", "softwareType": "dummy2"}
    Length Should Be    ${software}    2

sm-plugins download files from Cumulocity
    Execute Command    sudo tedge config set software.plugin.max_packages 1
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${file_url}=    Cumulocity.Create Inventory Binary    sm-plugin-test-file    software1    contents=Testing a thing
    ${OPERATION}=    Install Software    dummy-software,1.0.0::dummy3,${file_url}
    ${OPERATION}=    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    File Should Exist    /tmp/dummy3/installed_dummy-software
    ${downloaded}=    Execute Command    cat /tmp/dummy3/installed_dummy-software
    Should Be Equal    ${downloaded}    Testing a thing

Filter packages list using include pattern
    Execute Command    sudo tedge config set software.plugin.include "^dummy1-[0-1]00[2-4]$"
    Execute Command    sudo tedge config set software.plugin.max_packages 0
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software
    ...    {"name": "dummy1-0002", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0003", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0004", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-1002", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-1003", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-1004", "version": "1.0.0", "softwareType": "dummy1"}
    Length Should Be    ${software}    6

Filter packages list using exclude pattern
    Execute Command    sudo tedge config set software.plugin.exclude "^dummy[1-2]-\\d+(0|2|4|6|8)$"
    Execute Command    sudo tedge config set software.plugin.max_packages 0
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software
    ...    {"name": "dummy1-0001", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0003", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0005", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-1499", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy2-0001", "version": "1.0.0", "softwareType": "dummy2"}
    ...    {"name": "dummy2-0003", "version": "1.0.0", "softwareType": "dummy2"}
    ...    {"name": "dummy2-0005", "version": "1.0.0", "softwareType": "dummy2"}
    ...    {"name": "dummy2-1499", "version": "1.0.0", "softwareType": "dummy2"}
    Length Should Be    ${software}    1500

Filter packages list using both patterns
    Execute Command    sudo tedge config set software.plugin.exclude "^(dummy1.*)"
    Execute Command    sudo tedge config set software.plugin.include "^(dummy1-\\d(|3|6|9)00)$"
    Execute Command    sudo tedge config set software.plugin.max_packages 0
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software
    ...    {"name": "dummy1-0300", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0600", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-0900", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy1-1300", "version": "1.0.0", "softwareType": "dummy1"}
    ...    {"name": "dummy2-0001", "version": "1.0.0", "softwareType": "dummy2"}
    ...    {"name": "dummy2-1500", "version": "1.0.0", "softwareType": "dummy2"}
    Length Should Be    ${software}    1504

Software updates download software packages only once #3062
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${file_url}=    Cumulocity.Create Inventory Binary    sm-plugin-test-file    software1    contents=Testing a thing
    ${OPERATION}=    Install Software    dummy-software,1.0.0::dummy1,${file_url}
    ${OPERATION}=    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    ${op_id}=    Get From Dictionary    ${OPERATION}    id
    ${count}=    Execute Command    grep Downloading /var/log/tedge/agent/workflow-software_update-c8y-mapper-${op_id}.log -c
    Should Be Equal As Numbers    ${count}    1

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    # Remove any existing packages to allow for exact assertions
    Execute Command       rm -f /etc/tedge/sm-plugins/*
    Transfer To Device    ${CURDIR}/dummy-plugin.sh    /etc/tedge/sm-plugins/dummy1
    Transfer To Device    ${CURDIR}/dummy-plugin.sh    /etc/tedge/sm-plugins/dummy2
    Transfer To Device    ${CURDIR}/dummy-plugin-2.sh  /etc/tedge/sm-plugins/dummy3

Custom Teardown
    # Restore sudo in case if the tests are run on a device (and not in a container)
    Execute Command    [ -f /usr/bin/sudo.bak ] && mv /usr/bin/sudo.bak /usr/bin/sudo || true
    Get Logs

Set Service User
    [Arguments]    ${SERVICE_NAME}    ${SERVICE_USER}
    Execute Command    mkdir -p /etc/systemd/system/${SERVICE_NAME}.service.d/
    Execute Command    cmd=printf "[Service]\nUser = ${SERVICE_USER}" | sudo tee /etc/systemd/system/${SERVICE_NAME}.service.d/10-user.conf
    Execute Command    systemctl daemon-reload
