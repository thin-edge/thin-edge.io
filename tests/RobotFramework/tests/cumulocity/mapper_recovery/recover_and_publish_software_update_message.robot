*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:mapper_recovery


*** Test Cases ***
Mapper recovers and processes output of ongoing software update request
    [Documentation]    C8y-Mapper receives a software update request,
    ...    Delegates operation to tedge-agent and gets back `executing` status
    ...    And then goes down (here purposefully stopped).
    ...    Mean while the agent processes the update message and publishes the software update message
    ...    After some time mapper recovers and pushes the result to c8y cloud
    ...    Verify that the rolldice package is installed or not
    # Retry test until the root cause is fixed in https://github.com/thin-edge/thin-edge.io/issues/3773
    [Tags]    test:retry(3)    workaround
    ${timestamp}=    Get Unix Timestamp
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y
    ${OPERATION}=    Install Software    rolldice,1.0.0::dummy
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_update/+
    ...    message_contains=executing
    ...    date_from=${timestamp}
    ThinEdgeIO.Stop Service    tedge-mapper-c8y
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_update/+
    ...    message_contains=successful
    ...    date_from=${timestamp}
    ThinEdgeIO.Start Service    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=60
    Device Should Have Installed Software    rolldice

Recovery from corrupt entity store file
    Stop Service    tedge-agent
    Execute Command    chown root:root /etc/tedge/.agent/entity_store.jsonl
    Start Service    tedge-agent
    Service Should Be Running    tedge-agent
    ${owner}=    Execute Command    stat -c "%U" /etc/tedge/.agent/entity_store.jsonl    strip="true"
    Should Be Equal    ${owner}    tedge


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y
    # This acts as a custom sm plugin
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom_sw_plugin.sh    /etc/tedge/sm-plugins/dummy
