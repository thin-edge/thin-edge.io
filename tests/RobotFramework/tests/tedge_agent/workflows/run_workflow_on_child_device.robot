*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs    name=${PARENT_SN}

Test Tags           theme:tedge_agent


*** Variables ***
${PARENT_IP}    ${EMPTY}
${PARENT_SN}    ${EMPTY}
${CHILD_SN}     ${EMPTY}


*** Test Cases ***
Run workflow on child device
    Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

    Set Device Context    ${PARENT_SN}
    Should Have MQTT Messages
    ...    te/device/${CHILD_SN}///cmd/user-command
    ...    pattern="^{}$"
    Execute Command
    ...    tedge mqtt pub --retain te/device/${CHILD_SN}///cmd/user-command/child-test-1 '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/${CHILD_SN}///cmd/user-command/child-test-1
    ...    message_pattern=.*successful.*

    Set Device Context    ${CHILD_SN}
    ${workflow_log}=    Execute Command    cat /var/log/tedge/agent/workflow-user-command-child-test-1.log
    Should Contain    ${workflow_log}    item="user-command":"first-version"

Run service command on child device
    Set Device Context    ${CHILD_SN}
    ${pid_before}=    Get Service PID    sleeper

    Set Device Context    ${PARENT_SN}
    Register Entity    device/${CHILD_SN}/service/sleeper    service    device/${CHILD_SN}//
    Execute Command
    ...    tedge mqtt pub --retain te/device/${CHILD_SN}/service/sleeper/cmd/restart/child-service-1 '{"status":"init","serviceName":"sleeper","serviceType":"service"}'
    Should Have MQTT Messages
    ...    te/device/${CHILD_SN}/service/sleeper/cmd/restart/child-service-1
    ...    message_pattern=.*successful.*
    ...    maximum=1

    Set Device Context    ${CHILD_SN}
    ${pid_after}=    Get Service PID    sleeper
    Should Not Be Equal    ${pid_before}    ${pid_after}
    Execute Command    tedge mqtt pub --retain te/device/${CHILD_SN}/service/sleeper/cmd/restart/child-service-1 ''


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
    Execute Command    tedge config set http.bind.address 0.0.0.0
    Execute Command    tedge reconnect c8y
    Restart Service    tedge-agent

    # Child
    ${child_sn}=    Setup    skip_bootstrap=True
    Set Suite Variable    $CHILD_SN    ${child_sn}
    Set Device Context    ${CHILD_SN}

    Execute Command    dpkg -i packages/tedge_*.deb
    Execute Command    dpkg -i packages/tedge-agent_*.deb

    Execute Command    tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    tedge config set mqtt.client.port 1883
    Execute Command    tedge config set http.client.host ${PARENT_IP}
    Execute Command    tedge config set mqtt.topic_root te
    Execute Command    tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

    Transfer To Device    ${CURDIR}/echo-as-json.sh    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/user-command-v1.toml    /etc/tedge/operations/user-command.toml

    Transfer To Device    ${CURDIR}/service_commands/sleeper.service    /etc/systemd/system/
    Execute Command    systemctl daemon-reload && systemctl start sleeper

    Start Service    tedge-agent
