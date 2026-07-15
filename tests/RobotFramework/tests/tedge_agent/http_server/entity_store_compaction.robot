*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs    ${DEVICE_SN}

Test Tags           theme:tedge_agent


*** Variables ***
${DEVICE_SN}        ${EMPTY}
${ENTITY_STORE}     /etc/tedge/.agent/entity_store.jsonl


*** Test Cases ***
Entity store log is compacted when redundancy threshold is exceeded
    [Documentation]    Updating the same entity registration 101 times produces 100 redundant log
    ...    entries, which meets the compaction threshold. After compaction only one entry for
    ...    that topic is present in the file rather than 101.
    Write Entity Registration Many Times    compaction_count    101
    # The compaction might trigger earlier than expected as services may have reregistered on startup,
    # but the count should be well below the threshold of 100 redundant entries

    # do the comparison inside the command so grep is subject to retry if command completes before compaction
    Execute Command    cmd=bash -c '[ $(grep -c "compaction_count" ${ENTITY_STORE}) -lt 10 ]'    strip=${True}

Latest entity metadata is preserved after compaction
    [Documentation]    After compaction, entity metadata retains the most recently written
    ...    registration value, not an older one.
    Write Entity Registration Many Times    compaction_value    101
    Wait Until Keyword Succeeds
    ...    10s
    ...    500ms
    ...    Entity Twin Fragment Should Be
    ...    device/compaction_value//
    ...    name
    ...    101

Compacted entity store survives agent restart without data loss
    [Documentation]    After compaction the agent can be restarted and reload the compacted log
    ...    correctly. Entity metadata written before the restart remains accessible afterwards.
    Execute Command    sudo tedge config set agent.entity_store.clean_start false
    Write Entity Registration Many Times    compaction_restart    101
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    10s
    ...    500ms
    ...    Entity Twin Fragment Should Be
    ...    device/compaction_restart//
    ...    name
    ...    101
    [Teardown]    Restart Test Teardown

Twin fragments are not logged to the entity store
    [Documentation]    Direct twin fragment updates are retained MQTT state, but they are not
    ...    persisted in entity_store.jsonl.
    Write Twin Fragment Many Times    twin_not_logged    101
    ${count}=    Execute Command    grep -c "twin_not_logged" ${ENTITY_STORE} || true    strip=${True}
    Should Be Equal    ${count}    0

Twin fragments are restored from retained MQTT messages after restart
    [Documentation]    Direct twin fragment updates are restored from retained MQTT messages,
    ...    not from entity_store.jsonl.
    Execute Command    sudo tedge config set agent.entity_store.clean_start false
    Write Twin Fragment Many Times    retained_twin_not_logged    101
    ${count}=    Execute Command    grep -c "retained_twin_not_logged" ${ENTITY_STORE} || true    strip=${True}
    Should Be Equal    ${count}    0
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    10s
    ...    500ms
    ...    Entity Twin Fragment Should Be
    ...    device/main//
    ...    retained_twin_not_logged
    ...    101
    ${count}=    Execute Command    grep -c "retained_twin_not_logged" ${ENTITY_STORE} || true    strip=${True}
    Should Be Equal    ${count}    0
    [Teardown]    Restart Test Teardown

Writing to unique topics accumulates entries without triggering compaction
    [Documentation]    Registering child devices — each a unique topic — should not trigger
    ...    compaction. The log grows by exactly one entry per registration with no entries
    ...    silently removed.
    ${before}=    Execute Command    wc -l < ${ENTITY_STORE}    strip=${True}
    FOR    ${i}    IN RANGE    1    11
        Execute Command
        ...    curl -sf -X POST http://localhost:8000/te/v1/entities -H 'Content-Type: application/json' -d '{"@topic-id": "device/unique_child_${i}//", "@type": "child-device"}'
    END
    ${after}=    Execute Command    wc -l < ${ENTITY_STORE}    strip=${True}
    # Each registration adds one unique line; no redundancy means no compaction
    Should Be Equal    ${${after} + 0}    ${${before} + 10}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN

Write Twin Fragment Many Times
    [Arguments]    ${fragment}    ${count}
    Execute Command
    ...    for i in $(seq 1 ${count}); do tedge http put /te/v1/entities/device/main///twin/${fragment} "$i"; done

Write Entity Registration Many Times
    [Arguments]    ${child}    ${count}
    Execute Command
    ...    for i in $(seq 1 ${count}); do tedge mqtt pub --retain 'te/device/${child}//' '{"@type":"child-device","name":'"$i"'}'; done

Entity Twin Fragment Should Be
    [Arguments]    ${topic_id}    ${fragment}    ${expected}
    ${value}=    Execute Command
    ...    curl -s http://localhost:8000/te/v1/entities/${topic_id}/twin/${fragment}    strip=${True}
    Should Be Equal    ${value}    ${expected}

Restart Test Teardown
    Execute Command    sudo tedge config set agent.entity_store.clean_start true
    Get Logs    ${DEVICE_SN}
