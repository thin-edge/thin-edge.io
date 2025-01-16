*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Test Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:monitoring


*** Variables ***
${INTERVAL_CHANGE_TIMEOUT}      180
${CHECK_INTERVAL}               10
${HEARTBEAT_INTERVAL}           60


*** Test Cases ***
### Main Device ###
Heartbeat is sent
    [Documentation]    Full end-to-end test which will check the Cumulocity behaviour to sending the heartbeat signal
    ...    The tests therefore relies on the backend availability service which then sets the c8y_Availability fragment
    ...    based on the last received telemetry data.
    Device Should Have Fragment Values
    ...    c8y_RequiredAvailability.responseInterval\=1
    ...    timeout=5
    ...    wait=1
    Device Should Have Fragment Values
    ...    c8y_Availability.status\=AVAILABLE
    ...    timeout=${INTERVAL_CHANGE_TIMEOUT}
    ...    wait=${CHECK_INTERVAL}
    Stop Service    tedge-agent
    Service Health Status Should Be Down    tedge-agent
    Device Should Have Fragment Values
    ...    c8y_Availability.status\=UNAVAILABLE
    ...    timeout=${INTERVAL_CHANGE_TIMEOUT}
    ...    wait=${CHECK_INTERVAL}

    Start Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    Device Should Have Fragment Values
    ...    c8y_Availability.status\=AVAILABLE
    ...    timeout=${INTERVAL_CHANGE_TIMEOUT}
    ...    wait=${CHECK_INTERVAL}

#
# Note about remaining test cases
# The remaining test cases do not use the platform to assert whether the availability is set or not
# as this either takes too long, and is too flakey as the performance of the backend service which sets
# the c8y_Availability fragments varies greatly (as it is designed for longer intervals, e.g. ~30/60 mins)
# Instead, the test cases check if the heartbeat signal is being sent for the given devices which does not
# rely on any additional platform checks.
#

Heartbeat is sent based on the custom health topic status
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/main//' '{"@type":"device","@health":"device/main/service/foo"}'
    Execute Command    tedge mqtt pub --retain 'te/device/main/service/foo/status/health' '{"status":"up"}'
    ${existing_count}=    Should Have Heartbeat Message Count    ${DEVICE_SN}    minimum=1

    # Stop tedge-agent to make sure the heartbeat is not sent based on the tedge-agent status
    Stop Service    tedge-agent
    Service Health Status Should Be Down    tedge-agent
    ${existing_count}=    Should Have Heartbeat Message Count
    ...    ${DEVICE_SN}
    ...    minimum=${existing_count + 1}
    ...    timeout=${HEARTBEAT_INTERVAL}

### Child Device ###

Child heartbeat is sent
    # Register a child device
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device"}'
    Set Device    ${CHILD_XID}
    Device Should Exist    ${CHILD_XID}

    ${existing_count}=    Should Have Heartbeat Message Count    ${CHILD_XID}    minimum=0    maximum=0

    # Fake tedge-agent status is up for the child device
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/tedge-agent/status/health' '{"status":"up"}'
    Should Have Heartbeat Message Count
    ...    ${CHILD_XID}
    ...    minimum=${existing_count + 1}
    ...    timeout=${HEARTBEAT_INTERVAL}

Child heartbeat is sent based on the custom health topic status
    # Register a child device
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device", "@health":"device/${CHILD_SN}/service/bar"}'
    Set Device    ${CHILD_XID}
    Device Should Exist    ${CHILD_XID}

    ${existing_count}=    Should Have Heartbeat Message Count    ${CHILD_XID}    minimum=0    maximum=0

    # The custom health endpoint is up but tedge-agent is down
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/bar/status/health' '{"status":"up"}'
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/tedge-agent/status/health' '{"status":"down"}'

    Should Have Heartbeat Message Count
    ...    ${CHILD_XID}
    ...    minimum=${existing_count + 1}
    ...    timeout=${HEARTBEAT_INTERVAL}


*** Keywords ***
Test Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Test Variable    $DEVICE_SN

    ${CHILD_SN}=    Get Random Name
    Set Test Variable    $CHILD_SN
    Set Test Variable    $CHILD_XID    ${DEVICE_SN}:device:${CHILD_SN}

    # Set tedge config value before connecting
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect
    Execute Command    sudo tedge config set c8y.availability.interval 1m
    Execute Command    ./bootstrap.sh --no-install

    Device Should Exist    ${DEVICE_SN}

Should Have Heartbeat Message Count
    [Arguments]    ${SERIAL}    ${minimum}=${None}    ${maximum}=${None}    ${timeout}=30
    ${messages}=    Should Have MQTT Messages
    ...    c8y/inventory/managedObjects/update/${SERIAL}
    ...    minimum=${minimum}
    ...    maximum=${maximum}
    ...    message_pattern=^\{\}$
    ...    timeout=${timeout}
    ${count}=    Get Length    ${messages}
    RETURN    ${count}
