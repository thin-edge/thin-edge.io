*** Settings ***
Library             String
Library             ThinEdgeIO

Suite Setup         Custom Suite Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:troubleshooting    theme:cli    theme:plugins


*** Test Cases ***
01_tedge
    ${log_names}=    Create List
    ...    output.log
    ...    tedge-agent.log
    ...    tedge-mapper-c8y.log
    ...    tedge-mapper-collectd.log
    ...    tedge-config-list.log
    ...    tedge.toml
    FOR    ${log_name}    IN    @{log_names}
        File Size Is Not Zero    ${log_name}
    END

    Log Should Contain    tedge-agent.log    Starting tedge-agent.service
    Log Should Contain    tedge-mapper-c8y.log    Starting tedge-mapper-c8y.service
    Log Should Contain    tedge-mapper-collectd.log    Starting tedge-mapper-collectd.service
    Log Should Contain    tedge-config-list.log    c8y.url
    Execute Command    diff /etc/tedge/tedge.toml /results/test/01_tedge/tedge.toml

02_os
    File Size Is Not Zero    output.log
    Log Should Contain    output.log    Linux

03_mqtt
    ${log_names}=    Create List
    ...    output.log
    ...    tedge-mqtt-sub.log
    ...    tedge-mqtt-sub-retained-only.log
    FOR    ${log_name}    IN    @{log_names}
        File Size Is Not Zero    ${log_name}
    END
    Log Should Contain    tedge-mqtt-sub.log    [te/device/main///cmd/restart] {}
    Log Should Contain
    ...    tedge-mqtt-sub-retained-only.log
    ...    [te/device/main/service/tedge-agent] {"@parent":"device/main//","@type":"service","name":"tedge-agent","type":"service"}

04_workflow
    File Size Is Not Zero    output.log
    Execute Command    diff -qr /var/log/tedge/agent/workflow-* /results/test/04_workflow/workflow-*

05_entities
    File Size Is Not Zero    output.log
    Log Should Contain    output.log    "@topic-id": "device/main//"

06_internal
    ${log_names}=    Create List
    ...    output.log
    ...    entity_store.jsonl
    FOR    ${log_name}    IN    @{log_names}
        File Size Is Not Zero    ${log_name}
    END
    Execute Command    diff /etc/tedge/.agent/entity_store.jsonl /results/test/06_internal/entity_store.jsonl

07_mosquitto
    ${log_names}=    Create List
    ...    output.log
    ...    mosquitto.log
    ...    mosquitto/mosquitto.conf
    ...    mosquitto/conf.d/mosquitto.conf
    ...    mosquitto-journal.log
    ...    tedge/mosquitto-conf/tedge-mosquitto.conf
    ...    tedge/mosquitto-conf/c8y-bridge.conf
    FOR    ${log_name}    IN    @{log_names}
        File Size Is Not Zero    ${log_name}
    END
    Execute Command    diff /var/log/mosquitto/mosquitto.log /results/test/07_mosquitto/mosquitto.log
    Log Should Contain    mosquitto-journal.log    Starting mosquitto.service


*** Keywords ***
File Size Is Not Zero
    [Arguments]    ${filename}
    Execute Command    test -s /results/test/${TEST NAME}/${filename}

Log Should Contain
    [Arguments]    ${filename}    ${string}
    ${content}=    Execute Command    cat /results/test/${TEST NAME}/${filename}
    Should Contain    ${content}    ${string}

Custom Suite Setup
    Setup
    Execute Command    mkdir -p /results
    Start Service    tedge-mapper-collectd
    Execute Command    tedge diag collect --keep-dir --output-dir /results --name test
