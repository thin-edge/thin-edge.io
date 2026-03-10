*** Settings ***
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:tedge_flows


*** Variables ***
${FLOWS_DIR}    /etc/tedge/mappers/local/flows/
${FLOW_NAME}    hot-reload-flows
${FLOW_DIR}     ${FLOWS_DIR}${FLOW_NAME}/


*** Test Cases ***
Flow is reloaded after being touched many times
    [Documentation]    Regression test for https://github.com/thin-edge/thin-edge.io/issues/4028
    ...
    ...    Reloading a flow many times used to exhaust the QuickJS 16 MB runtime
    ...    memory limit because every reload accumulated module objects in
    ...    ctx->loaded_modules.    After ~40 reloads of a ~160 KB script the
    ...    mapper would start logging "Failed to compile flow: JS raised exception"
    ...    and refuse to process any more messages from that flow.
    ...
    ...    The fix recreates the AsyncContext on each hot-reload so that
    ...    JS_FreeContext releases all accumulated module memory before the new
    ...    version is compiled.
    [Tags]    theme:tedge_flows
    [Setup]    Install Hot Reload Flow
    # Trigger 200 hot-reloads by touching the toml; the bug surfaced around reload 40.
    Execute Command    cmd=for i in $(seq 1 500); do touch -m ${FLOW_DIR}flow.toml; sleep .1; done

    # Confirm the flow is still functional after all reloads.
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub test/hot-reload '{"temp": 42}'
    Should Have MQTT Messages
    ...    topic=test/hot-reload/out
    ...    minimum=1
    ...    message_contains=42
    ...    date_from=${start}
    [Teardown]    Execute Command    rm -rf ${FLOW_DIR}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN

    # FIXME: Remove directory creation and restarting of service once
    # https://github.com/thin-edge/thin-edge.io/issues/4029 is resolved
    Execute Command    mkdir -p ${FLOW_DIR}
    Restart Service    tedge-mapper-local

Install Hot Reload Flow
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/hot-reload-flows/*    ${FLOW_DIR}
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-local/status/flows
    ...    date_from=${start}
    ...    message_contains=${FLOW_NAME}
    ...    timeout=15
