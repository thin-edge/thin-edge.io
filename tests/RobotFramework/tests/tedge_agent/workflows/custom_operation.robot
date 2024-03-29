*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             OperatingSystem

Force Tags          theme:tedge_agent
Suite Setup         Custom Setup
Test Setup          Custom Test Setup
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

Trigger Device Restart Using A Sub-Command
    ${pid_before}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/restart_sub_command/robot-314 '{"status":"init"}'
    Should Have MQTT Messages    te/device/main///cmd/restart_sub_command/robot-314    message_pattern=.*successful.*   maximum=1    timeout=300
    ${pid_after}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Should Not Be Equal    ${pid_before}    ${pid_after}    msg=tedge-agent should have been restarted

Timeout An Action
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/slow_operation/robot-1 '{"status":"init"}'
    Should Have MQTT Messages    te/device/main///cmd/slow_operation/robot-1    message_pattern=.*timeout.*   maximum=1

Trigger Agent Restart
    ${pid_before}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/restart-tedge-agent/robot-1 '{"status":"init"}'
    Should Have MQTT Messages    te/device/main///cmd/restart-tedge-agent/robot-1    message_pattern=.*tedge-agent-restarted.*   minimum=1    timeout=300
    ${pid_after}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Should Not Be Equal    ${pid_before}    ${pid_after}    msg=tedge-agent should have been restarted

Trigger native-reboot within workflow (on_success)
    Execute Command    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /usr/bin/systemctl, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/sbin/reboot, /usr/bin/on_shutdown.sh' > /etc/sudoers.d/tedge
    ${pid_before}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/native-reboot/robot-1 '{"status":"init"}'
    Should Have MQTT Messages    te/device/main///cmd/native-reboot/robot-1    message_pattern=.*successful.*   maximum=1    timeout=300
    ${pid_after}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Should Not Be Equal    ${pid_before}    ${pid_after}    msg=tedge-agent should have been restarted
    ${workflow_log}=  Execute Command    cat /var/log/tedge/agent/workflow-native-reboot-robot-1.log
    Should Contain    ${workflow_log}    restarted:    msg=restarted state should have been executed

Trigger native-reboot within workflow (on_error) - missing sudoers entry for reboot
    Execute Command    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync' > /etc/sudoers.d/tedge
    ${pid_before}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Execute Command     tedge mqtt pub --retain te/device/main///cmd/native-reboot/robot-2 '{"status":"init"}'
    Should Have MQTT Messages    te/device/main///cmd/native-reboot/robot-2    message_pattern=.*failed.*   maximum=1    timeout=180
    ${pid_after}=  Execute Command    sudo systemctl show --property MainPID tedge-agent
    Should Be Equal    ${pid_before}    ${pid_after}    msg=tedge-agent should not have been restarted
    ${workflow_log}=  Execute Command    cat /var/log/tedge/agent/workflow-native-reboot-robot-2.log
    Should Not Contain    ${workflow_log}    restarted:    msg=restarted state should not have been executed

Invoke sub-command from a super-command operation
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/super_command/test-42 '{"status":"init", "output_file":"/tmp/test-42.json"}'
    ${cmd_messages}    Should Have MQTT Messages    te/device/main///cmd/super_command/test-42    message_pattern=.*successful.*   maximum=1
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/super_command/test-42 ''
    ${actual_log}      Execute Command    cat /tmp/test-42.json
    ${expected_log}    Get File    ${CURDIR}/super-command-expected.log
    Should Be Equal    ${actual_log}    ${expected_log}
    # Remove all dates from the workflow log
    ${workflow_log}=  Execute Command    cat /var/log/tedge/agent/workflow-super_command-test-42.log
    Should Contain    ${workflow_log}    super_command/test-42/init:
    Should Contain    ${workflow_log}    super_command/test-42/executing:
    Should Contain    ${workflow_log}    super_command/test-42/awaiting_completion:
    Should Contain    ${workflow_log}    sub_command/sub:super_command:test-42/init:                      msg=main command log should contain sub command steps
    Should Contain    ${workflow_log}    sub_command/sub:super_command:test-42/executing:                 msg=main command log should contain sub command steps
    Should Contain    ${workflow_log}    sub_command/sub:super_command:test-42/successful:                msg=main command log should contain sub command steps
    Should Contain    ${workflow_log}    super_command/test-42/successful:

Use scripts to create sub-command init states
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/lite_device_profile/test-42 '{"status":"init", "logfile":"/tmp/profile-42.log", "profile":"/etc/tedge/operations/lite_device_profile.example.txt"}'
    Should Have MQTT Messages    te/device/main///cmd/lite_device_profile/test-42    message_pattern=.*successful.*   maximum=1
    ${actual_log}      Execute Command    cat /tmp/profile-42.log
    ${expected_log}    Get File    ${CURDIR}/lite_device_profile.expected.log
    Should Be Equal    ${actual_log}    ${expected_log}

*** Keywords ***

Custom Test Setup
    Execute Command    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /usr/bin/systemctl, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh' > /etc/sudoers.d/tedge

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
    Copy Configuration Files
    Restart Service    tedge-agent

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
    ThinEdgeIO.Transfer To Device    ${CURDIR}/native-reboot.toml       /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/super_command.toml       /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/inner_command.toml       /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/echo-as-json.sh          /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/write-file.sh            /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/restart_sub_command.toml           /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/extract_updates.sh                 /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/lite_device_profile.toml           /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/lite_config_update.toml            /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/lite_software_update.toml          /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/lite_device_profile.example.txt    /etc/tedge/operations/
