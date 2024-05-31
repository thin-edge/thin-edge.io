*** Settings ***
Resource    ../../../resources/common.resource
Library    ThinEdgeIO

Suite Setup    Suite Setup

#
# Testing against older mosquitto versions is difficult as the there is not
# an easy way to installed older versions without changing the whole operating system
# (e.g. switching to Debian buster)
#
# The tests assume there is mosquitto 2.x running or newer given that this is the
# default in Debian bullseye and newer
#

*** Test Cases ***

Default local_cleansession setting
    Configure and Verify local_cleansession setting    default    local_cleansession false

Force inclusion of local_cleansession
    Configure and Verify local_cleansession setting    true    local_cleansession false

Auto detection of local_cleansession compatibility
    Configure and Verify local_cleansession setting    auto    local_cleansession false

Force exclusion of local_cleansession
    Configure and Verify local_cleansession setting    false    ${EMPTY}


*** Keywords ***

Configure and Verify local_cleansession setting
    [Arguments]    ${value}    ${expected_value}=${EMPTY}

    IF    $value == "default"
        Execute Command    tedge config unset c8y.bridge.include.local_cleansession
    ELSE
        Execute Command    tedge config set c8y.bridge.include.local_cleansession ${value}    
    END
    
    Execute Command    tedge reconnect c8y
    Execute Command    grep "^cleansession true" /etc/tedge/mosquitto-conf/c8y-bridge.conf
    ${local_cleansesion}=    Execute Command    grep "^local_cleansession " /etc/tedge/mosquitto-conf/c8y-bridge.conf    ignore_exit_code=${True}    strip=${True}
    Should Be Equal    ${local_cleansesion}    ${expected_value}

    # mosquitto should be running (to validate the configuration)
    Service Should Be Running    mosquitto

Suite Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}

    Execute Command    tedge config set mqtt.bridge.built_in false

    # Print which mosquitto version is being used
    Execute Command    mosquitto --help | head -1
