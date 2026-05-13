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
    [Documentation]    Writing to the same twin fragment 101 times produces 100 redundant log
    ...    entries, which meets the compaction threshold. After compaction only one entry for
    ...    that topic is present in the file rather than 101.
    Write Twin Fragment Many Times    compaction_count    101
    ${count}=    Execute Command    grep -c "compaction_count" ${ENTITY_STORE}    strip=${True}
    # The compaction might trigger earlier than expected as services may have reregistered on startup,
    # but the count should be well below the threshold of 100 redundant entries
    Should Be True    ${count} < 10

Latest twin value is preserved after compaction
    [Documentation]    After compaction the twin fragment retains the most recently written
    ...    value, not an older one.
    Write Twin Fragment Many Times    compaction_value    101
    ${value}=    Execute Command
    ...    curl -s http://localhost:8000/te/v1/entities/device/main///twin/compaction_value    strip=${True}
    Should Be Equal    ${value}    101

Compacted entity store survives agent restart without data loss
    [Documentation]    After compaction the agent can be restarted and reload the compacted log
    ...    correctly. Twin data written before the restart remains accessible afterwards.
    Execute Command    sudo tedge config set agent.entity_store.clean_start false
    Write Twin Fragment Many Times    compaction_restart    101
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    ${value}=    Execute Command
    ...    curl -s http://localhost:8000/te/v1/entities/device/main///twin/compaction_restart    strip=${True}
    Should Be Equal    ${value}    101
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

Restart Test Teardown
    Execute Command    sudo tedge config set agent.entity_store.clean_start true
    Get Logs    ${DEVICE_SN}
