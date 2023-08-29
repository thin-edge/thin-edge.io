*** Settings ***

Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

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
    ${software}=    Device Should Have Installed Software    dummy1-0001,1.0.0::dummy1    dummy1-0002,1.0.0::dummy1    dummy1-0003,1.0.0::dummy1    dummy1-0004,1.0.0::dummy1    dummy1-0005,1.0.0::dummy1    dummy2-0001,1.0.0::dummy2    dummy2-0002,1.0.0::dummy2    dummy2-0003,1.0.0::dummy2    dummy2-0004,1.0.0::dummy2    dummy2-0005,1.0.0::dummy2
    Length Should Be    ${software}    10

Limit number of packages to 1
    Execute Command    sudo tedge config set software.plugin.max_packages 1
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software    dummy1-0001,1.0.0::dummy1    dummy2-0001,1.0.0::dummy2
    Length Should Be    ${software}    2

Don't limit number of packages
    Execute Command    sudo tedge config set software.plugin.max_packages 0
    Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}
    ${software}=    Device Should Have Installed Software    dummy1-0001,1.0.0::dummy1    dummy2-0001,1.0.0::dummy2
    Length Should Be    ${software}    3000


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    # Remove any existing packages to allow for exact assertions
    Execute Command       rm -f /etc/tedge/sm-plugins/*
    Transfer To Device    ${CURDIR}/dummy-plugin.sh    /etc/tedge/sm-plugins/dummy1
    Transfer To Device    ${CURDIR}/dummy-plugin.sh    /etc/tedge/sm-plugins/dummy2

Custom Teardown
    Get Logs
