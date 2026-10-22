*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_agent    theme:software


*** Test Cases ***
Software list requests interrupted by an agent restart are run again
    ${start}=    Get Unix Timestamp

    # The first request is run by the software manager, the second waits in its queue
    Execute Command    touch /tmp/slow-list
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-running '{"status":"init"}'
    Execute Command    timeout 10 sh -c 'until [ -f /tmp/slow-list-started ]; do sleep 0.1; done'
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-queued '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/robot-queued
    ...    message_contains="executing"
    ...    date_from=${start}

    Execute Command    rm /tmp/slow-list
    Restart Service    tedge-agent

    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/robot-running
    ...    message_contains="successful"
    ...    date_from=${start}
    ...    timeout=60
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/robot-queued
    ...    message_contains="successful"
    ...    date_from=${start}
    ...    timeout=60
    Should Not Have MQTT Messages
    ...    te/device/main///cmd/software_list/+
    ...    message_contains="failed"
    ...    date_from=${start}

    [Teardown]    Clear Software List Commands


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    $DEVICE_SN
    Transfer To Device    ${CURDIR}/slow-list-plugin.sh    /etc/tedge/sm-plugins/slow
    Execute Command    chmod +x /etc/tedge/sm-plugins/slow
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

Clear Software List Commands
    Execute Command    rm -f /tmp/slow-list /tmp/slow-list-started
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-running ''
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/software_list/robot-queued ''
    Get Logs
