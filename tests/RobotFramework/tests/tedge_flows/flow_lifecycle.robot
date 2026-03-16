*** Settings ***
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs
Test Teardown       Remove Lifecycle Flow

Test Tags           theme:tedge_flows


*** Variables ***
${FLOWS_DIR}                /etc/tedge/mappers/local/flows
${FLOW_DIR}                 ${FLOWS_DIR}/lifecycle-test
${MOVED_FLOW_DIR}           /etc/tedge/mappers/local/lifecycle-test-moved
${FLOW_STATUS_TOPIC}        te/device/main/service/tedge-mapper-local/status/flows
${INPUT_TOPIC}              test/lifecycle/in
${OUTPUT_TOPIC}             test/lifecycle/out
${UPDATED_INPUT_TOPIC}      test/lifecycle/updated/in


*** Test Cases ***
Flow is added and processes messages
    [Documentation]    After installing a flow, messages sent to the input topic
    ...    must be transformed and appear on the output topic.
    Install Lifecycle Flow    main-v1.js

    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

Flow is removed and no longer processes messages
    [Documentation]    After removing a flow, messages sent to the former input topic
    ...    must no longer be processed or produce output.
    Install Lifecycle Flow    main-v1.js

    # Confirm the flow is active before removing it
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

    # Remove the flow and wait for it to unload
    Remove Lifecycle Flow
    Execute Command    sleep 1

    # Messages sent after removal must not appear on the output topic
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Not Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    date_from=${start}

Flow script update changes processing behavior
    [Documentation]    After replacing the JS script file, the flow is reloaded automatically.
    ...    Subsequent messages must be processed by the new script.
    Install Lifecycle Flow    main-v1.js

    # Verify v1 behavior
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

    # Overwrite the script with v2 and wait for the flow to reload
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/lifecycle-flows/main-v2.js    ${FLOW_DIR}/main.js
    Should Have MQTT Messages
    ...    topic=${FLOW_STATUS_TOPIC}
    ...    date_from=${start}
    ...    message_contains=lifecycle-test

    # Verify v2 behavior is now active
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v2"
    ...    date_from=${start}

Removing script unloads flow and restoring script loads flow again
    [Documentation]    After the JS script file is moved of the path referred by flow, the flow must be unloaded and loaded again when script is restored.
    Install Lifecycle Flow    main-v1.js

    # Verify v1 behavior
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

    # Move script to main flows dir
    Execute Command    sleep 1
    ${start}    Get Unix Timestamp
    Execute Command    mv ${FLOW_DIR}/main.js ${FLOWS_DIR}/main.js

    # FIXME: when we get Modified event for a js file we currently load it immediately even if it's not referred by any
    # loaded or unloaded flow. Then we try to load all unloaded flows, hoping loaded script fixed them, but if these
    # flows are not using the script, we emit this error message. Instead we shouldn't try to reload these flows unless
    # the script we're modifying is part of the flow, which will avoid this error message.
    Logs Should Contain    Failed to compile flow    date_from=${start}
    Should Have MQTT Messages
    ...    topic=${FLOW_STATUS_TOPIC}
    ...    date_from=${start}
    ...    message_contains="flow":"/etc/tedge/mappers/local/flows/lifecycle-test/main.js","status":"removed"

    # Move script back to the flow dir
    Execute Command    sleep 1
    Execute Command    mv ${FLOWS_DIR}/main.js ${FLOW_DIR}/main.js

    # Verify v1 behavior
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

Flow definition update changes processing behavior
    [Documentation]    After updating the flow definition (TOML file), the flow is reloaded
    ...    with the new settings. This test changes the input topic and verifies that the
    ...    flow responds to the new topic and ignores the old one.
    Install Lifecycle Flow    main-v1.js

    # Verify flow works on the original input topic
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

    # Overwrite the flow definition with v2 (different input topic) and wait for reload
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/lifecycle-flows/flow-v2.toml    ${FLOW_DIR}/flow.toml
    Should Have MQTT Messages
    ...    topic=${FLOW_STATUS_TOPIC}
    ...    date_from=${start}
    ...    message_contains=lifecycle-test

    # Messages on the new input topic must now be processed
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${UPDATED_INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

    # Brief settle: Get Unix Timestamp has 1-second granularity, so without this
    # sleep the output message from the UPDATED_INPUT_TOPIC publish (which was
    # confirmed above) can share the same second as the next ${start} and bleed
    # into the negative assertion window, causing a spurious failure.
    Execute Command    sleep 1

    # Messages on the original input topic must no longer be processed
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Not Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    date_from=${start}

Flow directory moved out on same filesystem is removed from engine
    [Documentation]    Regression test for the directory-move case: when a flow directory is
    ...    renamed/moved out of the watched flows directory on the same filesystem, the
    ...    kernel emits only a Modified event (not DirectoryDeleted), because it is a
    ...    rename operation. The engine must detect this and remove the flow from memory.
    Install Lifecycle Flow    main-v1.js

    # Confirm the flow is active before the move
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

    # Move the directory to a sibling location on the same filesystem.
    # This generates a Modified event on the parent directory (not DirectoryDeleted),
    # so on_path_updated must recognise it as a removal via the absent-path + flows-under-it check.
    Execute Command    mv ${FLOW_DIR} ${MOVED_FLOW_DIR}
    Execute Command    sleep 1

    # The flow must no longer process messages
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Not Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    date_from=${start}
    [Teardown]    Remove Moved Lifecycle Flow

Flow is reloaded after package-style replacement
    [Documentation]    Regression test for https://github.com/thin-edge/thin-edge.io/issues/4023.
    ...    Simulates a package manager update where individual flow files are deleted and
    ...    immediately recreated with new content. This generates file-level FileDeleted and
    ...    Modified inotify events (including the synthetic Modified that the notify crate
    ...    emits alongside every FileDeleted), which is the event sequence that triggered the
    ...    race condition in #4023.
    ...
    ...    A directory-rename approach (rm -rf + mv) must NOT be used here because it produces
    ...    directory-level events that bypass the file-level race conditions entirely.
    ...
    ...    After the replacement the test sleeps to give any stale FileDeleted events time to
    ...    be delivered and processed. With the bug present, that stale FileDeleted would
    ...    remove the freshly-reloaded flow. With the fix, the flow survives the wait.
    Install Lifecycle Flow    main-v1.js

    # Confirm v1 is active
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v1"
    ...    date_from=${start}

    # Stage v2 files outside the watched directory so that the staging transfers
    # do not generate events in the flows watcher.
    Execute Command    mkdir -p /etc/tedge/mappers/local/lifecycle-test-staging
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/lifecycle-flows/main-v2.js
    ...    /etc/tedge/mappers/local/lifecycle-test-staging/main.js
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/lifecycle-flows/flow.toml
    ...    /etc/tedge/mappers/local/lifecycle-test-staging/flow.toml

    # Simulate package install: delete existing flow files then immediately copy the new
    # versions in. All in a single shell invocation so the events land in the same (or
    # adjacent) inotify debounce windows. This generates file-level events:
    #    FileDeleted(flow.toml) + synthetic Modified(flow.toml)
    #    FileDeleted(main.js)    + synthetic Modified(main.js)
    #    Modified(flow.toml) from cp
    #    Modified(main.js)    from cp
    # With Bug 1, the synthetic Modified fires on_path_removed when !exists → removes flow.
    # With Bug 2, a stale FileDeleted arriving after the reload removes the flow again.
    ${start}    Get Unix Timestamp
    Execute Command
    ...    rm -f ${FLOW_DIR}/flow.toml ${FLOW_DIR}/main.js && cp /etc/tedge/mappers/local/lifecycle-test-staging/flow.toml ${FLOW_DIR}/flow.toml && cp /etc/tedge/mappers/local/lifecycle-test-staging/main.js ${FLOW_DIR}/main.js

    # Wait for the flow to be acknowledged as reloaded (the status message may correspond
    # to a removal event — that is intentional; we care about the final state after the sleep).
    Should Have MQTT Messages
    ...    topic=${FLOW_STATUS_TOPIC}
    ...    date_from=${start}
    ...    message_contains=lifecycle-test

    # Critical: give any stale inotify events (e.g. a delayed FileDeleted) enough time to
    # be delivered and processed. With the bug from #4023, a stale FileDeleted would remove
    # the flow from memory even after it had been reloaded — this sleep exposes that window.
    Execute Command    sleep 1

    # After the stale-event window, the flow must still be loaded and processing messages.
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub ${INPUT_TOPIC} '{}'
    Should Have MQTT Messages
    ...    topic=${OUTPUT_TOPIC}
    ...    message_contains="version":"v2"
    ...    date_from=${start}
    [Teardown]    Remove Package Replace Test Artifacts


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Restart Service    tedge-mapper-local

Install Lifecycle Flow
    [Documentation]    Install the lifecycle test flow using the given JS file as main.js.
    ...    Transfers the script first, then the flow definition to trigger loading.
    ...    Waits for the status/flows MQTT confirmation before returning.
    [Arguments]    ${script}
    # Wait for the service to report healthy before transferring files.
    # systemctl restart returns as soon as the unit is "active", but the inotify
    # file-watchers are registered slightly later. Without this wait, the Transfer
    # events can arrive before the watcher is ready and the flow never loads.
    Service Health Status Should Be Up    tedge-mapper-local

    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/lifecycle-flows/${script}    ${FLOW_DIR}/main.js
    ThinEdgeIO.Transfer To Device    ${CURDIR}/lifecycle-flows/flow.toml    ${FLOW_DIR}/flow.toml
    Should Have MQTT Messages
    ...    topic=${FLOW_STATUS_TOPIC}
    ...    date_from=${start}
    ...    message_contains=lifecycle-test

Remove Lifecycle Flow
    [Documentation]    Remove the lifecycle test flow directory. Safe to call even if the
    ...    directory does not exist (uses rm -rf).
    Execute Command    cmd=rm -rf ${FLOW_DIR}

Remove Moved Lifecycle Flow
    [Documentation]    Teardown for the same-filesystem directory move test. Removes the
    ...    flow directory from wherever it ended up (original or moved location).
    Execute Command    rm -rf ${FLOW_DIR} ${MOVED_FLOW_DIR}

Remove Package Replace Test Artifacts
    [Documentation]    Teardown for the package-style replacement test. Removes both the
    ...    live flow directory and the staging directory used during the test.
    Remove Lifecycle Flow
    Execute Command    rm -rf /etc/tedge/mappers/local/lifecycle-test-staging
