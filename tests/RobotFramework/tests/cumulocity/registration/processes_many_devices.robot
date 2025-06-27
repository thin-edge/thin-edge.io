*** Settings ***
Documentation       A separate suite for #3279, because a lot of devices need to be registered, doesn't connect to the
...                 cloud. Attempt to register many devices and check that messages are still processed.

Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Test Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:monitoring    \#3279


*** Variables ***
${NUM_ENTITIES}     ${100}


*** Test Cases ***
Processes many devices
    Stop Service    tedge-mapper-c8y

    Execute Command
    ...    for i in $(seq 0 ${NUM_ENTITIES}); do tedge mqtt pub --retain "te/device/child$i//" '{"@type":"child-device"}'; done

    Start Service    tedge-mapper-c8y

    FOR    ${index}    IN RANGE    ${NUM_ENTITIES}
        Should Have MQTT Messages    c8y/s/us    message_pattern=101,.*child${index}
    END


*** Keywords ***
Test Setup
    ${DEVICE_SN}=    Setup

    # Don't actually send topics c8y/... to the cloud, the test is 100% local
    Execute Command    sed -i s/c8y/asdf/ /etc/tedge/mosquitto-conf/c8y-bridge.conf
    Restart Service    mosquitto
