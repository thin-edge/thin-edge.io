*** Settings ***
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:configuration


*** Test Cases ***
Config operation plugin
    # Install lighttpd as the test software
    Execute Command    sudo apt install -y lighttpd
    Service Should Be Running    lighttpd

    # Install lighttpd config management plugin
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/lighttpd
    ...    /usr/share/tedge/config-plugins/lighttpd
    Execute Command    chmod +x /usr/share/tedge/config-plugins/lighttpd

    Should Contain Supported Configuration Types    lighttpd.conf::lighttpd

    # Get current configuration
    ${operation}=    Cumulocity.Get Configuration    lighttpd.conf::lighttpd
    Operation Should Be SUCCESSFUL    ${operation}    timeout=10

    # Verify initial server tag
    ${initial_tag}=    Execute Command
    ...    curl -I http://localhost 2>/dev/null | grep -i '^Server:'
    Should Contain    ${initial_tag}    lighttpd

    # Apply new configuration with custom server tag
    ${config_url}=    Cumulocity.Create Inventory Binary
    ...    lighttpd.conf
    ...    lighttpd.conf
    ...    file=${CURDIR}/plugins/lighttpd.conf
    ${operation}=    Cumulocity.Set Configuration    lighttpd.conf::lighttpd    url=${config_url}
    Operation Should Be SUCCESSFUL    ${operation}    timeout=30

    # Verify server tag has been updated
    ${updated_tag}=    Execute Command    curl -I http://localhost 2>/dev/null | grep -i '^Server:'
    Should Contain    ${updated_tag}    tedge-lighttpd


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Execute Command    mkdir /usr/share/tedge/config-plugins
    Restart Service    tedge-agent
    Service Should Be Running    tedge-agent
