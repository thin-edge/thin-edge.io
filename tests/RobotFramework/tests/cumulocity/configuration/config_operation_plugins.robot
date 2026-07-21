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
    ${OPERATION}=    Install Software    {"name": "cron", "softwareType": "apt"}
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

    # Verify operation stdout is logged
    ${operation_logfile}=    Execute Command
    ...    ls -t /var/log/tedge/agent/workflow-config_snapshot-* | head -1
    ...    strip=True
    Execute Command    grep "Dummy content" ${operation_logfile}    exp_exit_code=0    retries=0    timeout=0

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

Config operation shouldnt OOM on oversized output
    [Documentation]    Install a plugin that emits a very large config and verify all of its output is not loaded into
    ...                memory at once.

    # install custom config plugin that generates lots of data on stdout, here 50MB
    # but it shouldn't send all of it to c8y, so after outputting all this to stdout we print short
    # message on stderr and exit.
    # tedge shouldn't send any stdout/stderr data to the cloud until the plugin process exits.
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/huge
    ...    /usr/share/tedge/config-plugins/huge
    Execute Command    chmod +x /usr/share/tedge/config-plugins/huge

    # request config
    Should Contain Supported Configuration Types    huge::huge
    ${operation}=    Cumulocity.Get Configuration    huge::huge

    # Wait until huge plugin creates a pipe which means stdout was filled and we can measure memory usage
    Execute Command    ls /tmp/huge-config-plugin-pipe    retries=5    timeout=2

    ${tedge_agent_rss_bytes}=    Execute Command    cmd=ps -o rss= -C tedge-agent    strip=True
    Should Be True
    ...    ${tedge_agent_rss_bytes} < 25000
    ...    tedge-agent used too much (${tedge_agent_rss_bytes}>25000KiB) memory

    # send message to plugin to exit
    Execute Command    echo done > /tmp/huge-config-plugin-pipe

    # make sure operation completed and tedge-agent is still running
    # (?s:.) matches any character regardless of flags
    # https://docs.python.org/3/library/re.html
    Operation Should Be FAILED    ${operation}    failure_reason=(?s:.)*script generated(?s:.)*
    Execute Command    systemctl is-active tedge-agent


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
