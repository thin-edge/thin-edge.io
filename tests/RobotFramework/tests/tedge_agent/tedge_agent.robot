*** Comments ***
# PRECONDITION:
# Device CH_DEV_CONF_MGMT is existing on tenant, if not
# use -v DeviceID:xxxxxxxxxxx in the command line to use your existing device


*** Settings ***
Resource            ../../resources/common.resource
Library             Collections
Library             JSONLibrary
Library             ThinEdgeIO
Library             Cumulocity

Test Setup          Custom Setup
Test Teardown       Get Logs    name=${PARENT_SN}

Test Tags           theme:tedge_agent


*** Variables ***
${PARENT_IP}    ${EMPTY}
${PARENT_SN}    ${EMPTY}
${CHILD_SN}     ${EMPTY}


*** Test Cases ***
File transfer service are not running on a child device
    Set Device Context    ${CHILD_SN}

    # check that file transfer service is disabled
    # 7 - Failed to connect to host
    Execute Command
    ...    curl -X PUT -d '' http://127.0.0.1:8000/te/v1/files/test-file
    ...    exp_exit_code=7

Tedge-agent restarts cleanly without timeout
    [Documentation]    Regression test for \#4041
    ...    TwinManagerActor used to deadlock on shutdown, causing a 60s timeout

    ${start_time}=    Get Unix Timestamp
    Restart Service    tedge-agent
    Restart Service    tedge-agent
    Service Should Be Running    tedge-agent
    ${end_time}=    Get Unix Timestamp

    ${JOURNAL_LOG}=    Execute Command    journalctl -u tedge-agent --since "@${start_time}" --no-pager
    Should Not Contain    ${JOURNAL_LOG}    ERROR Runtime: Timeout waiting for all actors to shutdown

    ${elapsed}=    Evaluate    ${end_time} - ${start_time}
    Should Be True    ${elapsed} < 60
    ...    msg=tedge-agent took ${elapsed}s to restart, expected less than 60s


*** Keywords ***
Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=False
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}

    Set Device Context    ${PARENT_SN}
    Execute Command    tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    tedge config set mqtt.external.bind.port 1883
    Execute Command    tedge reconnect c8y

    # Child
    ${child_sn}=    Setup    skip_bootstrap=True
    Set Suite Variable    $CHILD_SN    ${child_sn}
    Set Device Context    ${CHILD_SN}
    Execute Command    echo '[mqtt]' >> /etc/tedge/tedge.toml
    Execute Command    echo 'device_topic_id \= "device/child1//"' >> /etc/tedge/tedge.toml
    Execute Command    echo 'client.host \= "${PARENT_IP}"' >> /etc/tedge/tedge.toml

    # Install and start tedge-agent
    Execute Command    dpkg -i packages/tedge_*.deb
    Execute Command    dpkg -i packages/tedge-agent_*.deb
    Start Service    tedge-agent
    # delay for tedge-agent to connect to parent device MQTT broker
    Sleep    3s

    Set Device Context    ${PARENT_SN}
    # we check that tedge-agent on child device was able to connect
    Execute Command    grep -q "Sending CONNACK to tedge-agent#te/device/child1//" /var/log/mosquitto/mosquitto.log
