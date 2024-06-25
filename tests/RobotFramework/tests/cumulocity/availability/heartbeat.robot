*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:monitoring
Test Setup    Test Setup
Test Teardown    Get Logs


*** Variables ***

${INTERVAL_CHANGE_TIMEOUT}    120
${CHECK_INTERVAL}             10

*** Test Cases ***

### Main Device ###
Heartbeat is sent
    Device Should Have Fragment Values    c8y_Availability.status\=AVAILABLE   timeout=${INTERVAL_CHANGE_TIMEOUT}    wait=${CHECK_INTERVAL}
    Stop Service    tedge-agent
    Service Health Status Should Be Down    tedge-agent
    Device Should Have Fragment Values    c8y_Availability.status\=UNAVAILABLE   timeout=${INTERVAL_CHANGE_TIMEOUT}    wait=${CHECK_INTERVAL}

    Start Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    Device Should Have Fragment Values    c8y_Availability.status\=AVAILABLE   timeout=${INTERVAL_CHANGE_TIMEOUT}    wait=${CHECK_INTERVAL}

Heartbeat is sent based on the custom health topic status
    Execute Command    tedge mqtt pub --retain 'te/device/main//' '{"@health":"device/main/service/foo"}'
    Execute Command    tedge mqtt pub --retain 'te/device/main/service/foo/status/health' '{"status":"up"}'

    # Stop tedge-agent to make sure the heartbeat is not sent based on the tedge-agent status
    Stop Service    tedge-agent
    Service Health Status Should Be Down    tedge-agent

    Sleep    90s    reason=Wait for the server to have updated the status
    Device Should Have Fragment Values    c8y_Availability.status\=AVAILABLE

### Child Device ###
Child heartbeat is sent
    # Register a child device
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device"}'
    Set Device    ${CHILD_XID}
    Device Should Exist    ${CHILD_XID}

    Device Should Have Fragment Values    c8y_Availability.status\=UNAVAILABLE    timeout=${INTERVAL_CHANGE_TIMEOUT}    wait=${CHECK_INTERVAL}

    # Fake tedge-agent status is up for the child device
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/tedge-agent/status/health' '{"status":"up"}'
    Device Should Have Fragment Values    c8y_Availability.status\=AVAILABLE    timeout=${INTERVAL_CHANGE_TIMEOUT}    wait=${CHECK_INTERVAL}

Child heartbeat is sent based on the custom health topic status
    # Register a child device
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device", "@health":"device/${CHILD_SN}/service/bar"}'
    Set Device    ${CHILD_XID}
    Device Should Exist    ${CHILD_XID}

    # The custom health endpoint is up but tedge-agent is down
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/bar/status/health' '{"status":"up"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/tedge-agent/status/health' '{"status":"down"}'

    Sleep    90s    reason=Wait for the server to have updated the status
    Device Should Have Fragment Values    c8y_Availability.status\=AVAILABLE


*** Keywords ***
Test Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Test Variable     $DEVICE_SN

    ${CHILD_SN}=    Get Random Name
    Set Test Variable    $CHILD_SN
    Set Test Variable    $CHILD_XID    ${DEVICE_SN}:device:${CHILD_SN}

    # Set tedge config value before connecting
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect
    Execute Command    sudo tedge config set c8y.availability.interval 1m
    Execute Command    ./bootstrap.sh --no-install

    Device Should Exist    ${DEVICE_SN}
