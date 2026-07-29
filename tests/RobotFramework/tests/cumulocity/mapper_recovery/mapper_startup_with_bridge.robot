*** Settings ***
Documentation       Anything the mapper publishes to the cloud topics before the bridge relays them is
...                 discarded by the local broker rather than forwarded. The mapper therefore waits for
...                 the bridge before connecting to the broker, which also leaves whatever was published
...                 to it meanwhile queued in its session instead of consumed and dropped.
...                 These tests use the built-in bridge, as it starts within the mapper process and so
...                 races with it on every mapper restart.

Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:mapper_recovery


*** Test Cases ***
Supported operations reach the cloud when the mapper restarts alongside the bridge
    [Documentation]    The mapper announces the operations it supports as soon as it starts. On a fresh
    ...    session the bridge holds no subscription on the cloud topics yet, so that announcement is
    ...    lost unless the mapper waits for it.
    # Leave the cloud with a truncated list, so only an announcement made after the restart restores it
    Execute Command    tedge mqtt pub c8y/s/us '114,c8y_Restart'
    Cumulocity.Should Not Contain Supported Operations    c8y_SoftwareUpdate

    ${timestamp}=    Get Unix Timestamp
    ThinEdgeIO.Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

    Should Have MQTT Messages    c8y/s/us    message_contains=114,    date_from=${timestamp}
    Cumulocity.Should Contain Supported Operations    c8y_Restart    c8y_SoftwareUpdate

Telemetry published while the mapper is down reaches the cloud once it restarts
    [Documentation]    A measurement published while the mapper is down stays queued in the mapper's
    ...    session on the local broker. Holding back the connection rather than the conversion keeps it
    ...    there until the bridge can relay what it converts to, so a mapper restarted meanwhile — as
    ...    when the bridge is being set up — still delivers it.
    ThinEdgeIO.Stop Service    tedge-mapper-c8y

    Execute Command    tedge mqtt pub te/device/main///m/queued_while_down '{"temperature":21.5}'

    ThinEdgeIO.Start Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

    Cumulocity.Device Should Have Measurements
    ...    type=queued_while_down
    ...    value=temperature
    ...    series=temperature
    ...    minimum=1
    ...    maximum=1


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge reconnect c8y
    Device Should Exist    ${DEVICE_SN}
    Service Health Status Should Be Up    tedge-mapper-c8y
