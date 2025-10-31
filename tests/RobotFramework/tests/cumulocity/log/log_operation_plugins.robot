*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             OperatingSystem
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log


*** Test Cases ***
Log operation journald plugin
    Should Contain Supported Log Types    tedge-agent::journald
    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=tedge-agent::journald
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}
    Log Operation Attachment File Contains
    ...    ${operation}
    ...    expected_pattern=.*Starting tedge-agent.*

Supported log types updated on software update
    ${start_time}=    Get Unix Timestamp
    ${OPERATION}=    Install Software    cron
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60

    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/log_upload
    ...    date_from=${start_time}
    ...    message_contains=cron::journald
    Should Contain Supported Log Types    cron::journald

Supported log types updated on config update
    [Documentation]    Updating any configuration should trigger supported log types update
    ${config_url}=    Cumulocity.Create Inventory Binary
    ...    tedge-configuration-plugin
    ...    tedge-configuration-plugin
    ...    contents=files=[]
    ${start_time}=    Get Unix Timestamp
    ${operation}=    Cumulocity.Set Configuration    tedge-configuration-plugin    url=${config_url}
    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/log_upload
    ...    date_from=${start_time}
    ...    message_contains=software-management

Supported log types updated on sync signal
    Install Package Using APT    haveged

    ${start_time}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main/service/tedge-agent/signal/sync '{}'

    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/log_upload
    ...    date_from=${start_time}
    ...    message_contains=haveged::journald

Supported log types updated on sync log_upload signal
    Install Package Using APT    anacron

    ${start_time}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub te/device/main/service/tedge-agent/signal/sync_log_upload '{}'

    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/log_upload
    ...    date_from=${start_time}
    ...    message_contains=anacron::journald

Log operation journald plugin can return logs for all units
    Should Contain Supported Log Types    all-units::journald
    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=all-units::journald
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}
    Log Operation Attachment File Contains
    ...    ${operation}
    ...    expected_pattern=.*

Time range filtering
    # Test with specific time range that triggers filtering in fake_plugin
    # Using Unix timestamps: 1640995320 (2022-01-01 00:02:00) to 1640995620 (2022-01-01 00:07:00)
    ${start_timestamp}=    Set Variable    2022-01-01T00:02:00+0000
    ${end_timestamp}=    Set Variable    2022-01-01T00:07:00+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=fake_log::fake_plugin
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=30
    Log File Contents Should Be Equal
    ...    ${operation}
    ...    1640995320 [INFO] info log 2\n1640995380 [WARN] warn log 1\n1640995440 [INFO] info log 3\n1640995500 [ERROR] error log 1\n1640995560 [INFO] info log 4\n

Plugin search text filtering
    # Test search text filtering - should only find lines containing "ERROR"
    ${start_timestamp}=    Set Variable    2022-01-01T00:00:00+0000
    ${end_timestamp}=    Set Variable    2022-01-01T00:10:00+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=fake_log::fake_plugin
    ...    search_text=ERROR
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=30
    Log File Contents Should Be Equal
    ...    ${operation}
    ...    1640995500 [ERROR] error log 1\n1640995680 [ERROR] error log 2\n

Tail lines filtering
    # Test maximum lines filtering - should only get 3 lines
    ${start_timestamp}=    Set Variable    2022-01-01T00:00:00+0000
    ${end_timestamp}=    Set Variable    2022-01-01T00:10:00+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=fake_log::fake_plugin
    ...    maximum_lines=3
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=30
    Log File Contents Should Be Equal
    ...    ${operation}
    ...    1640995620 [DEBUG] debug log 2\n1640995680 [ERROR] error log 2\n1640995740 [INFO] info log 5\n

Combined filtering
    # Test combined filtering - search for "info" with maximum 2 lines between 1640995320 (2022-01-01 00:02:00) to 1640995620 (2022-01-01 00:07:00)
    ${start_timestamp}=    Set Variable    2022-01-01T00:02:00+0000
    ${end_timestamp}=    Set Variable    2022-01-01T00:07:00+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=fake_log::fake_plugin
    ...    search_text=info
    ...    maximum_lines=2
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=30
    Log File Contents Should Be Equal
    ...    ${operation}
    ...    1640995440 [INFO] info log 3\n1640995560 [INFO] info log 4\n

Non-existent plugin
    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=some_log::non_existent_plugin
    ${operation}=    Operation Should Be FAILED
    ...    ${operation}
    ...    timeout=120
    ...    failure_reason=.*Plugin not found.*

Dynamic plugin install and remove
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/dummy_plugin
    ...    /usr/share/tedge/log-plugins/dummy_plugin
    Execute Command    chmod +x /usr/share/tedge/log-plugins/dummy_plugin
    Should Contain Supported Log Types    dummy_log::dummy_plugin

    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=dummy_log::dummy_plugin
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log Operation Attachment File Contains
    ...    ${operation}
    ...    expected_pattern=.*Dummy content.*

    # Dynamically remove the plugin and verify subsequent operations for that plugin fails
    Execute Command    rm /usr/share/tedge/log-plugins/dummy_plugin

    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=dummy_log::dummy_plugin
    ${operation}=    Operation Should Be FAILED
    ...    ${operation}
    ...    timeout=120
    ...    failure_reason=.*Plugin not found.*

Overriding a log plugin
    # Add an extra location for local log plugins
    Execute Command    mkdir -p /usr/local/tedge/log-plugins
    Execute Command    tedge config set log.plugin_paths '/usr/local/tedge/log-plugins,/usr/share/tedge/log-plugins'
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/local/tedge/log-plugins/[a-zA-Z0-9]*' | sudo tee -a /etc/sudoers.d/tedge
    Restart Service    tedge-agent
    Should Contain Supported Log Types    fake_log::fake_plugin
    # Override the fake plugin with a dummy plugin
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/dummy_plugin
    ...    /usr/local/tedge/log-plugins/fake_plugin
    Execute Command    chmod a+x /usr/local/tedge/log-plugins/fake_plugin
    ${dummy_types}=    Execute Command    /usr/local/tedge/log-plugins/fake_plugin list
    Should Be Equal    ${dummy_types}    dummy_log    strip_spaces=${True}
    # The fake plugin has been overridden
    Should Contain Supported Log Types    dummy_log::fake_plugin

    ${start_timestamp}=    Get Current Date    UTC    -1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${end_timestamp}=    Get Current Date    UTC    +1 hours    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Create Log Request Operation
    ...    ${start_timestamp}
    ...    ${end_timestamp}
    ...    log_type=dummy_log::fake_plugin
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
    Log Operation Attachment File Contains
    ...    ${operation}
    ...    expected_pattern=.*Dummy content.*

Reporting sudo misconfiguration
    # Add an extra location for local log plugins
    # BUT failing to authorize tedge to run these plugins with sudo
    Stop Service    tedge-agent
    Execute Command    mkdir -p /etc/share/tedge/log-plugins
    Execute Command    tedge config add log.plugin_paths /etc/share/tedge/log-plugins
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/dummy_plugin
    ...    /etc/share/tedge/log-plugins/dummy_plugin
    Execute Command    chmod a+x /etc/share/tedge/log-plugins/dummy_plugin

    Start Service    tedge-agent
    Service Logs Should Contain
    ...    tedge-agent
    ...    current_only=${True}
    ...    text=ERROR log plugins: Skipping /etc/share/tedge/log-plugins/dummy_plugin: not properly configured to run with sudo

Agent resilient to plugin dirs removal
    ${date_from}=    Get Unix Timestamp
    Execute Command    rm -rf /usr/share/tedge/log-plugins
    Should Have MQTT Messages    c8y/s/us    date_from=${date_from}    message_pattern=118,

    ${date_from}=    Get Unix Timestamp
    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
    Should Have MQTT Messages    c8y/s/us    date_from=${date_from}    message_pattern=118,


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/fake_plugin
    ...    /usr/share/tedge/log-plugins/fake_plugin
    Execute Command    chmod +x /usr/share/tedge/log-plugins/fake_plugin

Create Log Request Operation
    [Arguments]    ${start_timestamp}
    ...    ${end_timestamp}
    ...    ${log_type}
    ...    ${search_text}=${EMPTY}
    ...    ${maximum_lines}=1000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"${start_timestamp}","dateTo":"${end_timestamp}","logFile":"${log_type}","searchText":"${search_text}","maximumLines":${maximum_lines}}}
    RETURN    ${operation}

Log Operation Attachment File Contains
    [Arguments]    ${operation}    ${expected_pattern}
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    ${event_id}=    Set Variable    ${event_url_parts}[-2]
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${event_id}
    ...    expected_pattern=${expected_pattern}
    ...    encoding=utf-8
    RETURN    ${contents}

Log File Contents Should Be Equal
    [Arguments]    ${operation}    ${expected_contents}
    ${event_url_parts}=    Split String    ${operation["c8y_LogfileRequest"]["file"]}    separator=/
    ${event_id}=    Set Variable    ${event_url_parts}[-2]
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${event_id}
    ...    expected_contents=${expected_contents}
    ...    encoding=utf-8
    ${event}=    Cumulocity.Event Attachment Should Have File Info
    ...    ${event_id}
    RETURN    ${contents}
