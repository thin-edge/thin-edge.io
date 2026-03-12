*** Settings ***
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:tedge_flows


*** Variables ***
${FLOWS_DIR}    /etc/tedge/mappers/local/flows


*** Test Cases ***
Flow is reloaded after being touched many times
    [Documentation]    Regression test for https://github.com/thin-edge/thin-edge.io/issues/4028
    ...
    ...    Reloading a flow many times used to exhaust the QuickJS runtime
    ...    memory limit. Ensure the flow is processing messages afterwards
    Install Hot Reload Flow

    # Trigger hot-reloads by touching the toml
    Execute Command    cmd=for i in $(seq 1 100); do touch -m ${FLOWS_DIR}/hot-reload/flow.toml; sleep .1; done

    # Confirm the flow is still functional after all reloads.
    ${start}    Get Unix Timestamp
    Execute Command    tedge mqtt pub test/hot-reload '{"temp": 42}'
    Should Have MQTT Messages
    ...    topic=test/hot-reload/out
    ...    minimum=1
    ...    message_contains=42
    ...    date_from=${start}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Enable Service    tedge-mapper-local
    Start Service    tedge-mapper-local
    Set Suite Variable    $DEVICE_SN

Install Hot Reload Flow
    ${start}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/hot-reload/*    ${FLOWS_DIR}/hot-reload/
    Should Have MQTT Messages
    ...    topic=te/device/main/service/tedge-mapper-local/status/flows
    ...    date_from=${start}
    ...    message_contains=hot-reload
    ...    timeout=15
