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

Supported config types updated on software update
    ${start_time}=    Get Unix Timestamp
    ${OPERATION}=    Install Software    cron
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60

    Command Metadata Should Have Refreshed    ${start_time}

Supported config types updated on sync signal
    ${start_time}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main/service/tedge-agent/signal/sync '{}'
    Command Metadata Should Have Refreshed    ${start_time}

    ${start_time}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main/service/tedge-agent/signal/sync_config '{}'
    Command Metadata Should Have Refreshed    ${start_time}

Dynamic plugin install and remove
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/dummy_plugin
    ...    /usr/share/tedge/config-plugins/dummy_plugin
    Execute Command    chmod +x /usr/share/tedge/config-plugins/dummy_plugin

    Should Contain Supported Configuration Types    dummy_config::dummy_plugin

    ${operation}=    Cumulocity.Get Configuration    dummy_config::dummy_plugin
    Operation Should Be SUCCESSFUL    ${operation}    timeout=30

    ${config_url}=    Cumulocity.Create Inventory Binary
    ...    dummy_config
    ...    dummy_config
    ...    contents=dummy content
    ${operation}=    Cumulocity.Set Configuration    dummy_config::dummy_plugin    url=${config_url}
    Operation Should Be SUCCESSFUL    ${operation}    timeout=30

    # Dynamically remove the plugin and verify subsequent operations fail
    Execute Command    rm /usr/share/tedge/config-plugins/dummy_plugin

    ${operation}=    Cumulocity.Get Configuration    dummy_config::dummy_plugin
    ${operation}=    Operation Should Be FAILED
    ...    ${operation}
    ...    timeout=30
    ...    failure_reason=.*Plugin not found.*

Demo Test
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/file
    ...    /usr/share/tedge/config-plugins/file
    Execute Command    chmod +x /usr/share/tedge/config-plugins/file

    ThinEdgeIO.Transfer To Device    ${CURDIR}/composite_config_update.toml    /etc/tedge/operations/

    Execute Command    touch /etc/tedge/test.conf
    Execute Command    echo "original config" >> /etc/tedge/test.conf
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/tedge-configuration-plugin-with-service.toml
    ...    /etc/tedge/plugins/tedge-configuration-plugin.toml
    Should Contain Supported Configuration Types    test-conf

    Execute Command
    ...    curl -X PUT --data-binary 'updated config' "http://localhost:8000/te/v1/files/${DEVICE_SN}/config_update/op-123"
    Execute Command
    ...    tedge mqtt pub -r te/device/main///cmd/config_update/op-123 '{"status":"init","tedgeUrl":"http://localhost:8000/te/v1/files/${DEVICE_SN}/config_update/op-123","remoteUrl":"","serverUrl":"","type":"test-conf"}'

    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/config_update/op-123
    ...    message_contains="status":"successful"


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

Command Metadata Should Have Refreshed
    [Arguments]    ${start_time}
    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/config_snapshot
    ...    date_from=${start_time}
    ...    message_contains=tedge-configuration-plugin
    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/config_update
    ...    date_from=${start_time}
    ...    message_contains=tedge-configuration-plugin
