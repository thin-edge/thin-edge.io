*** Settings ***
Resource            ../../../resources/common.resource
Library             Collections
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:c8y    theme:software    theme:plugins


*** Test Cases ***
Supported software types should be declared during startup
    [Documentation]    c8y_SupportedSoftwareTypes should NOT be created by default #2654
    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/software_list
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="types":["apt"]
    Should Have MQTT Messages
    ...    topic=te/device/main///cmd/software_update
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="types":["apt"]
    Run Keyword And Expect Error    *    Device Should Have Fragment Values    c8y_SupportedSoftwareTypes\=["apt"]

Supported software types and c8y_SupportedSoftwareTypes should be declared during startup
    [Documentation]    c8y_SupportedSoftwareTypes should be created if the relevant config is set to true #2654
    Execute Command    tedge config set c8y.software_management.with_types true
    Restart Service    tedge-mapper-c8y
    Device Should Have Fragment Values    c8y_SupportedSoftwareTypes\=["apt"]

Software list should be populated during startup
    [Documentation]    The list is sent via HTTP by default.
    Device Should Have Installed Software    tedge    timeout=120

Software list should be populated during startup with advanced software management
    [Documentation]    The list is sent via SmartREST with advanced software management. See #2654
    Execute Command    tedge config set c8y.software_management.api advanced
    Restart Service    tedge-mapper-c8y
    # Note: Only check minimum message count, as it is possible that the mapper isn't finished
    # processing previous created software_list operations before the tedge-mapper-c8y service
    # is restarted which results in more than 1 c8y/s/us 140 message being sent
    # See related flaky test issue #3426
    Should Have MQTT Messages    c8y/s/us    message_contains=140,    minimum=1
    Device Should Have Installed Software    tedge    timeout=120

Install software via Cumulocity
    ${OPERATION}=    Install Software    c8y-remote-access-plugin    # TODO: Use different package
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software    c8y-remote-access-plugin

tedge-agent should terminate on SIGINT while downloading file
    [Documentation]    The test uses a custom local http server with throttling applied to it to ensure
    ...    the download does not complete before stopping the tedge-agent
    ${start_time}=    Get Unix Timestamp
    ${OPERATION}=    Install Software    test-very-large-software,1.0,http://localhost/speedlimit/10MB

    # wait for the download to start by waiting for a specific marker to appear in the logs
    Logs Should Contain    text=download::download: Downloading file from url    date_from=${start_time}
    Operation Should Not Be PENDING    ${OPERATION}

    # Service should stop within 5s
    Stop tedge-agent

Software list should only show currently installed software and not candidates
    ${EXPECTED_VERSION}=    Execute Command    dpkg -s tedge | grep "^Version: " | cut -d' ' -f2    strip=True
    ${VERSION}=    Escape Pattern    ${EXPECTED_VERSION}    is_json=${True}
    Device Should Have Installed Software
    ...    {"name": "tedge", "softwareType": "apt", "version": "${VERSION}"}
    ...    timeout=120

Manual software_list operation request
    # Note: There isn't a Cumulocity operation related to getting the software list, so no need to check for operation transitions
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/software_list/local-1111
    ...    payload={"status":"init"}
    ...    expected_status=successful

Manual software_update operation request
    # Don't worry about the command failing, that is expected since the package to be installed does not exist
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/software_update/local-2222
    ...    payload={"status":"init","updateList":[{"type":"apt","modules":[{"name":"package-does-not-exist","version":"latest","action":"install"}]}]}
    ...    expected_status=failed
    ...    c8y_fragment=c8y_SoftwareUpdate

Manual software_update operation request with empty url
    Publish and Verify Local Command
    ...    topic=te/device/main///cmd/software_update/local-3333
    ...    payload={"status":"init","updateList":[{"type":"apt","modules":[{"name":"tedge","version":"latest","url":"","action":"install"}]}]}
    ...    expected_status=successful
    ...    c8y_fragment=c8y_SoftwareUpdate

Operation log uploaded automatically with default auto_log_upload setting as on-failure
    ${default_value}=    Execute Command    tedge config get c8y.operations.auto_log_upload    strip=${True}
    Should Be Equal    ${default_value}    on-failure

    # Validate that the operation log is NOT uploaded for a successful operation
    ${OPERATION}=    Install Software    c8y-remote-access-plugin
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Validate operation log not uploaded

    # Validate that the operation log is uploaded for a failed operation
    ${OPERATION}=    Install Software    non-existent-package
    Operation Should Be FAILED    ${OPERATION}    timeout=60
    Validate operation log uploaded

Operation log uploaded automatically with auto_log_upload setting as always
    Execute Command    tedge config set c8y.operations.auto_log_upload always
    Restart Service    tedge-mapper-c8y

    # Validate that the operation log is uploaded for a successful operation as well
    ${OPERATION}=    Install Software    c8y-remote-access-plugin
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Validate operation log uploaded

    # Validate that the operation log is uploaded for a failed operation
    ${OPERATION}=    Install Software    non-existent-package
    Operation Should Be FAILED    ${OPERATION}    timeout=60
    Validate operation log uploaded

Operation log uploaded automatically with auto_log_upload setting as never
    Execute Command    tedge config set c8y.operations.auto_log_upload never
    Restart Service    tedge-mapper-c8y

    # Validate that the operation log is NOT uploaded for a successful operation
    ${OPERATION}=    Install Software    c8y-remote-access-plugin
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Validate operation log not uploaded

    # Validate that the operation log is NOT uploaded for a failed operation either
    ${OPERATION}=    Install Software    non-existent-package
    Operation Should Be FAILED    ${OPERATION}    timeout=60
    Validate operation log not uploaded

Workflow log includes plugin output
    ${OPERATION}=    Install Software    c8y-remote-access-plugin
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    ${operation_log_file}=    Execute Command
    ...    ls -t /var/log/tedge/agent/workflow-software_update-* | head -n 1
    ...    strip=${True}
    ${log_output}=    Execute Command    cat ${operation_log_file}
    Should Contain    ${log_output}    Executing command: "apt-get" "--quiet" "--yes" "update"

Backward compatibility using 501-503 templates to update status
    Execute Command    tedge config set c8y.smartrest.use_operation_id false
    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y
    ${OPERATION}=    Install Software    c8y-remote-access-plugin
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software    c8y-remote-access-plugin
    Should Have MQTT Messages    c8y/s/us    message_pattern=^(501|502|503),c8y_SoftwareUpdate.*    minimum=1

Operation gets updated regardless of the order of the creation time
    Stop Service    tedge-agent
    ${OPERATION1}=    Install Software    non-existent-package1
    ${OPERATION2}=    Install Software    non-existent-package2
    ${op1dict}=    Operation Should Be PENDING    ${OPERATION1}
    ${op2dict}=    Operation Should Be PENDING    ${OPERATION2}
    ${op_id1}=    Get From Dictionary    ${op1dict}    id
    ${op_id2}=    Get From Dictionary    ${op2dict}    id
    Execute Command
    ...    tedge mqtt pub -r te/device/main///cmd/software_update/c8y-mapper-${op_id2} '{"status":"executing","updateList":[{"type":"default","modules":[{"name":"non-existent-package2","action":"install"}]}]}'
    Operation Should Be PENDING    ${OPERATION1}    timeout=60    # The older operation status shouldn't be updated
    Operation Should Be EXECUTING    ${OPERATION2}    timeout=60
    Execute Command
    ...    tedge mqtt pub -r te/device/main///cmd/software_update/c8y-mapper-${op_id2} '{"status":"successful","updateList":[{"type":"default","modules":[{"name":"non-existent-package2","action":"install"}]}]}'
    Operation Should Be SUCCESSFUL    ${OPERATION2}    timeout=60
    Execute Command
    ...    tedge mqtt pub -r te/device/main///cmd/software_update/c8y-mapper-${op_id1} '{"status":"executing","updateList":[{"type":"default","modules":[{"name":"non-existent-package1","action":"install"}]}]}'
    Operation Should Be EXECUTING    ${OPERATION1}    timeout=60
    Execute Command
    ...    tedge mqtt pub -r te/device/main///cmd/software_update/c8y-mapper-${op_id1} '{"status":"successful","updateList":[{"type":"default","modules":[{"name":"non-existent-package1","action":"install"}]}]}'
    Operation Should Be SUCCESSFUL    ${OPERATION1}    timeout=60

Agent should ignore unknown software-update fields
    # Issue https://github.com/thin-edge/thin-edge.io/issues/3136
    # Add the softwareType field erroneously to an item in the modules array
    Execute Command
    ...    tedge mqtt pub -r te/device/main///cmd/software_update/test-3136 '{"status":"init","updateList":[{"type":"apt","modules":[{"name":"jq","action":"install","version":"latest","softwareType":"invalid"}]}]}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_update/test-3136
    ...    message_pattern=.*"status":"successful".*
    ...    timeout=60


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}
    Set Test Variable    $DEVICE_SN
    Should Have MQTT Messages    te/device/main/service/tedge-mapper-c8y/status/health
    Execute Command    sudo start-http-server.sh

Stop tedge-agent
    [Timeout]    5 seconds
    Stop Service    tedge-agent

Custom Teardown
    Execute Command    sudo stop-http-server.sh
    Get Logs

Publish and Verify Local Command
    [Arguments]    ${topic}    ${payload}    ${expected_status}=successful    ${c8y_fragment}=
    Execute Command    tedge mqtt pub --retain '${topic}' '${payload}'
    ${messages}=    Should Have MQTT Messages
    ...    ${topic}
    ...    minimum=1
    ...    maximum=1
    ...    message_contains="status":"${expected_status}"

    Sleep    5s    reason=Given mapper a chance to react, if it does not react with 5 seconds it never will
    ${retained_message}=    Execute Command
    ...    timeout 1 tedge mqtt sub --no-topic '${topic}'
    ...    ignore_exit_code=${True}
    ...    strip=${True}
    Should Be Equal    ${messages[0]}    ${retained_message}    msg=MQTT message should be unchanged

    IF    "${c8y_fragment}"
        # There should not be any c8y related operation transition messages sent: https://cumulocity.com/docs/smartrest/mqtt-static-templates/#updating-operations
        Should Have MQTT Messages
        ...    c8y/s/us
        ...    message_pattern=^(501|502|503|504|505|506),${c8y_fragment}.*
        ...    minimum=0
        ...    maximum=0
    END
    [Teardown]    Execute Command    tedge mqtt pub --retain '${topic}' ''

Validate operation log uploaded
    # Find the latest workflow log for software update operation
    ${operation_log_file}=    Execute Command
    ...    ls -t /var/log/tedge/agent/workflow-software_update-* | head -n 1
    ...    strip=${True}
    ${log_checksum}=    Execute Command    md5sum '${operation_log_file}' | cut -d' ' -f1    strip=${True}
    ${events}=    Cumulocity.Device Should Have Event/s
    ...    minimum=1
    ...    type=software_update_op_log
    ...    with_attachment=${True}
    ${contents}=    Cumulocity.Event Should Have An Attachment
    ...    ${events[0]["id"]}
    ...    encoding=utf8
    ...    expected_pattern=.*wait for the requester to finalize the command.*
    Log    ${contents}

Validate operation log not uploaded
    ${events}=    Cumulocity.Device Should Have Event/s
    ...    minimum=0
    ...    maximum=0
    ...    type=software_update_op_log
