*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             OperatingSystem

Force Tags          theme:tedge_agent
Suite Setup         Custom Setup
Test Teardown       Get Logs

*** Test Cases ***

Trigger Custom Download Operation
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/download/robot-123 '{"status":"init","url":"https://from/there","file":"/put/it/here"}'
    ${cmd_messages}    Should Have MQTT Messages    te/device/main///cmd/download/robot-123    message_pattern=.*successful.*   maximum=1
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/download/robot-123 ''
    ${actual_log}      Execute Command    cat /tmp/download-robot-123
    ${expected_log}    Get File    ${CURDIR}/download-command-expected.log
    Should Be Equal    ${actual_log}    ${expected_log}

Override Built-In Operation
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/software_list/robot-456 '{"status":"init"}'
    ${software_list}    Should Have MQTT Messages    te/device/main///cmd/software_list/robot-456    message_pattern=.*successful.*   maximum=1
    Should Contain      ${software_list[0]}    "currentSoftwareList"
    Should Contain      ${software_list[0]}    "mosquitto"
    Should Contain      ${software_list[0]}    "tedge"
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/software_list/robot-456 ''

Trigger Device Restart
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/controlled_restart/robot-789 '{"status":"init"}'
    ${cmd_outcome}      Should Have MQTT Messages    te/device/main///cmd/controlled_restart/robot-789    message_pattern=.*successful.*   maximum=2
    ${actual_log}       Execute Command    cat /etc/tedge/operations/restart-robot-789
    ${expected_log}     Get File    ${CURDIR}/restart-command-expected.log
    Should Be Equal     ${actual_log}    ${expected_log}

Timeout An Action
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/slow_operation/robot-1 '{"status":"init"}'
    Should Have MQTT Messages    te/device/main///cmd/slow_operation/robot-1    message_pattern=.*timeout.*   maximum=1

Trigger Agent Restart
    ${pid_before}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/restart-tedge-agent/robot-1 '{"status":"init"}'
    Should Have MQTT Messages    te/device/main///cmd/restart-tedge-agent/robot-1    message_pattern=.*tedge-agent-restarted.*   minimum=1    timeout=300
    ${pid_after}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Should Not Be Equal    ${pid_before}    ${pid_after}    msg=tedge-agent should have been restarted

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
    Copy Configuration Files
    Restart Service    tedge-agent
    Execute Command    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /usr/bin/systemctl, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh' > /etc/sudoers.d/tedge

Copy Configuration Files
    ThinEdgeIO.Transfer To Device    ${CURDIR}/software_list.toml       /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/init-software-list.sh    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom-download.toml     /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/schedule-download.sh     /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/launch-download.sh       /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/check-download.sh        /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/custom_restart.toml      /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/log-restart.sh           /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/slow-operation.toml      /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/restart-tedge-agent.toml    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-agent-pid.sh       /etc/tedge/operations/
