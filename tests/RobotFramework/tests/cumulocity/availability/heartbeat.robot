*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y
Test Setup    Test Setup
Test Teardown    Get Logs

*** Test Cases ***

### Main Device ###
Heartbeat is sent
    # How to test if a heartbeat is sent?
    # c8y has time lag to change the availability state
    # that may cause false-negative result if we check c8y_Availability.status
    # also putting 'sleep 2m' a couple of times is not good :/

    Stop Service    tedge-agent
    Service Health Status Should Be Down    tedge-agent
    Sleep    2m
    Device Should Have Fragment Values    c8y_Availability.status=UNAVAILABLE

    Start Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    Sleep    2m
    Device Should Have Fragment Values    c8y_Availability.status=AVAILABLE

Heartbeat is sent based on the custom health topic status
    Execute Command    tedge mqtt pub --retain 'te/device/main//' '{"@health":"custom/health/topic"}'
    Execute Command    tedge mqtt pub --retain 'te/custom/health/topic' '{"status":"up"}'

    # Stop tedge-agent to make sure the heartbeat is not sent based on the tedge-agent status
    Stop Service    tedge-agent
    Service Health Status Should Be Down    tedge-agent

    Sleep    2m
    Device Should Have Fragment Values    c8y_Availability.status=AVAILABLE


### Child Device ###
Child heartbeat is sent
    # Register a child device
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device"}'
    Set Device    ${CHILD_XID}
    Device Should Exist    ${CHILD_XID}

    Sleep    2m
    Device Should Have Fragment Values    c8y_Availability.status=UNAVAILABLE

    # Fake tedge-agent status is up for the child device
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/tedge-agent/status/health' '{"status":"up"}'
    Sleep    2m
    Device Should Have Fragment Values    c8y_Availability.status=AVAILABLE

Child heartbeat is sent based on the custom health topic status
    # Register a child device
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device", "@health":"custom/child/health/topic"}'
    Set Device    ${CHILD_XID}
    Device Should Exist    ${CHILD_XID}

    # The custom health endpoint is up but tedge-agent is down
    Execute Command    tedge mqtt pub --retain 'te/custom/child/health/topic' '{"status":"up"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/tedge-agent/status/health' '{"status":"down"}'

    Sleep    2m
    Device Should Have Fragment Values    c8y_Availability.status=AVAILABLE


*** Keywords ***
Test Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Test Variable     $DEVICE_SN

    ${CHILD_SN}=    Get Random Name
    Set Test Variable    $CHILD_SN
    Set Test Variable    $CHILD_XID    ${DEVICE_SN}:device:${CHILD_SN}

    # Set tedge config value before connecting
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect
    Execute Command    sudo tedge config set c8y.availability.period 1
    Execute Command    ./bootstrap.sh --no-install

    Device Should Exist    ${DEVICE_SN}
