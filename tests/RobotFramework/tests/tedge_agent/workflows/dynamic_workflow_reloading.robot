*** Settings ***
Resource            ../../../resources/common.resource
Library             OperatingSystem
Library             JSONLibrary
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Custom Setup
Test Setup          Custom Test Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_agent


*** Test Cases ***
Create User-Defined Operation
    ThinEdgeIO.File Should Not Exist    /etc/tedge/operations/user-command.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/user-command-v1.toml    /etc/tedge/operations/user-command.toml
    ${capability}    Should Have MQTT Messages    te/device/main///cmd/user-command
    Should Be Equal    ${capability[0]}    {}
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/user-command/dyn-test-1 '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/user-command/dyn-test-1
    ...    message_pattern=.*successful.*
    ${workflow_log}    Execute Command    cat /var/log/tedge/agent/workflow-user-command-dyn-test-1.log
    Should Contain
    ...    ${workflow_log}
    ...    item="@version":"37d0861e3038b34e8ab2ffe3257dd9372213ed5e17ba352e5028b0bf9762a089"
    Should Contain    ${workflow_log}    item="user-command":"first-version"

Update User-Defined Operation
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/user-command.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/user-command-v2.toml    /etc/tedge/operations/user-command.toml
    ${capability}    Should Have MQTT Messages    te/device/main///cmd/user-command
    Should Be Equal    ${capability[0]}    {}
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/user-command/dyn-test-2 '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/user-command/dyn-test-2
    ...    message_pattern=.*successful.*
    ${workflow_log}    Execute Command    cat /var/log/tedge/agent/workflow-user-command-dyn-test-2.log
    Should Contain
    ...    ${workflow_log}
    ...    item="@version":"1370727b2fcd269c91546e36651b9c727897562a5d3cc8e861a1e35f09ec82a6"
    Should Contain    ${workflow_log}    item="user-command":"second-version"

Remove User-Defined Operation
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/user-command.toml
    ${timestamp}    Get Unix Timestamp
    Execute Command    rm /etc/tedge/operations/user-command.toml
    ${capability}    Should Have MQTT Messages    te/device/main///cmd/user-command    date_from=${timestamp}
    Should Be Empty    ${capability[0]}

Updating A Workflow Twice Before Using It
    ThinEdgeIO.File Should Not Exist    /etc/tedge/operations/user-command.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/user-command-v1.toml    /etc/tedge/operations/user-command.toml
    ${capability}    Should Have MQTT Messages    te/device/main///cmd/user-command
    Should Be Equal    ${capability[0]}    {}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/user-command-v2.toml    /etc/tedge/operations/user-command.toml
    ${capability}    Should Have MQTT Messages    te/device/main///cmd/user-command
    Should Be Equal    ${capability[0]}    {}
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/user-command/dyn-test-3 '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/user-command/dyn-test-3
    ...    message_pattern=.*successful.*
    ${workflow_log}    Execute Command    cat /var/log/tedge/agent/workflow-user-command-dyn-test-3.log
    Should Contain
    ...    ${workflow_log}
    ...    item="@version":"1370727b2fcd269c91546e36651b9c727897562a5d3cc8e861a1e35f09ec82a6"
    Should Contain    ${workflow_log}    item="user-command":"second-version"

Override Builtin Operation
    ThinEdgeIO.File Should Not Exist    /etc/tedge/operations/software_list.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/software_list.toml    /etc/tedge/operations/software_list.toml
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/software_list/dyn-test-4 '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/dyn-test-4
    ...    message_pattern=.*successful.*
    ${workflow_log}    Execute Command    cat /var/log/tedge/agent/workflow-software_list-dyn-test-4.log
    Should Contain
    ...    ${workflow_log}
    ...    item="@version":"76e9afe834b4a7cadc9029670ba76745fcda73784f9e78c09f0c0416f7f58ad2"

Recover Builtin Operation
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/software_list.toml
    Execute Command    rm /etc/tedge/operations/software_list.toml
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/software_list/dyn-test-5 '{"status":"init"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/dyn-test-5
    ...    message_pattern=.*successful.*
    ${workflow_log}    Execute Command    cat /var/log/tedge/agent/workflow-software_list-dyn-test-5.log
    Should Contain    ${workflow_log}    item="@version":"builtin"

Trigger Workflow Update From A Main Workflow
    # Enable user-command v1 and prepare v2
    ThinEdgeIO.Transfer To Device    ${CURDIR}/user-command-v1.toml    /etc/tedge/operations/user-command.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/user-command-v2.toml    /etc/tedge/operations/user-command.toml.v2
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/update-user-command.toml
    ...    /etc/tedge/operations/update-user-command.toml
    ${capability}    Should Have MQTT Messages    te/device/main///cmd/update-user-command
    Should Be Equal    ${capability[0]}    {}
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/update-user-command/dyn-test-6 '{"status":"init", "version":"v2"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/update-user-command/dyn-test-6
    ...    message_pattern=.*successful.*
    ${workflow_log}    Execute Command    cat /var/log/tedge/agent/workflow-update-user-command-dyn-test-6.log
    Should Contain
    ...    ${workflow_log}
    ...    item="user_command_version":"1370727b2fcd269c91546e36651b9c727897562a5d3cc8e861a1e35f09ec82a6"
    Should Contain    ${workflow_log}    item="user-command":"second-version"

Trigger Workflow Creation From A Main Workflow
    # Assuming the update-user-command workflow is already installed
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/update-user-command.toml
    # Fully disable user-command
    ${timestamp}    Get Unix Timestamp
    Execute Command    rm /etc/tedge/operations/user-command.toml
    Should Have MQTT Messages    te/device/main///cmd/user-command    pattern="^$"    date_from=${timestamp}
    # Prepare the creation of the user-command from the update-user-command workflow
    ThinEdgeIO.Transfer To Device    ${CURDIR}/user-command-v1.toml    /etc/tedge/operations/user-command.toml.v1
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/update-user-command/dyn-test-7 '{"status":"init", "version":"v1"}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/update-user-command/dyn-test-7
    ...    message_pattern=.*successful.*
    ${workflow_log}    Execute Command    cat /var/log/tedge/agent/workflow-update-user-command-dyn-test-7.log
    Should Contain
    ...    ${workflow_log}
    ...    item="user_command_version":"37d0861e3038b34e8ab2ffe3257dd9372213ed5e17ba352e5028b0bf9762a089"
    Should Contain    ${workflow_log}    item="user-command":"first-version"

Update Concurrently Running Versions
    Update Workflow    ${CURDIR}/sleep.toml    sleep
    # Trigger a first version of a long running command
    Update Workflow    ${CURDIR}/long-running-command-v1.toml    long-running-command
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/long-running-command/dyn-test-8 '{"status":"init", "duration":30}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/long-running-command/dyn-test-8
    ...    message_pattern=.*scheduled.*

    # Then a second version of the same long running command
    Update Workflow    ${CURDIR}/long-running-command-v2.toml    long-running-command
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/long-running-command/dyn-test-9 '{"status":"init", "duration":30}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/long-running-command/dyn-test-9
    ...    message_pattern=.*scheduled.*

    # And a third one
    Update Workflow    ${CURDIR}/long-running-command-v3.toml    long-running-command
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/long-running-command/dyn-test-10 '{"status":"init", "duration":30}'
    Should Have MQTT Messages
    ...    te/device/main///cmd/long-running-command/dyn-test-10
    ...    message_pattern=.*scheduled.*

    # Check the 3 workflows use their original workflow version till the end
    Should Have MQTT Messages
    ...    te/device/main///cmd/long-running-command/dyn-test-8
    ...    message_pattern=.*first-version.*
    ...    timeout=60
    Should Have MQTT Messages
    ...    te/device/main///cmd/long-running-command/dyn-test-9
    ...    message_pattern=.*second-version.*
    ...    timeout=60
    Should Have MQTT Messages
    ...    te/device/main///cmd/long-running-command/dyn-test-10
    ...    message_pattern=.*third-version.*
    ...    timeout=60

Resume On Restart A Pending Operation Which Workflow Is Deprecated
    # Trigger a long running command
    Update Workflow    ${CURDIR}/sleep.toml    sleep
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/sleep/dyn-test-11 '{"status":"init", "duration":30}'

    # Stop the agent, once sure the command is executing
    Should Have MQTT Messages
    ...    te/device/main///cmd/sleep/dyn-test-11
    ...    message_pattern=.*executing.*
    Stop Service    tedge-agent

    # Make sure the long running command has not been fully executed
    ${workflow_log}    Execute Command    cat /var/log/tedge/agent/workflow-sleep-dyn-test-11.log
    Should Not Contain
    ...    ${workflow_log}
    ...    item="logging"

    # Deprecate the long running command, and restart
    Execute Command    rm /etc/tedge/operations/sleep.toml
    Start Service    tedge-agent

    # The pending long command should resume, despite the operation has been deprecated
    ${messages}    Should Have MQTT Messages
    ...    te/device/main///cmd/sleep/dyn-test-11
    ...    message_pattern=.*successful.*
    ...    timeout=60
    Should Contain    ${messages[0]}    item="what a long sleep"

Resume On Restart A Pending Operation
    # Trigger a long running operation
    Update Workflow    ${CURDIR}/sleep-command.toml    sleep
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/sleep/dyn-test-12 '{"status":"init", "duration":120}'

    # Restart the agent, once sure the command is executing
    Should Have MQTT Messages
    ...    te/device/main///cmd/sleep/dyn-test-12
    ...    message_pattern=.*executing.*
    Restart Service    tedge-agent

    # The command should be interrupted and marked as failed
    ${messages}    Should Have MQTT Messages
    ...    te/device/main///cmd/sleep/dyn-test-12
    ...    message_pattern=.*failed.*
    ...    timeout=60
    Should Contain    ${messages[0]}    item="sleep killed by signal 15"
    Should Contain    ${messages[0]}    item="resumed_at"


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    Copy Scripts

Custom Test Setup
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/user-command/dyn-test-1 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/user-command/dyn-test-2 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/user-command/dyn-test-3 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/software_list/dyn-test-4 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/software_list/dyn-test-5 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/update-user-command/dyn-test-6 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/update-user-command/dyn-test-7 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/long-running-command/dyn-test-8 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/long-running-command/dyn-test-9 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/long-running-command/dyn-test-10 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/sleep/dyn-test-11 ''
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/sleep/dyn-test-12 ''

Copy Scripts
    ThinEdgeIO.Transfer To Device    ${CURDIR}/echo-as-json.sh    /etc/tedge/operations/

Update Workflow
    [Arguments]    ${FILE}    ${OPERATION}
    ${timestamp}    Get Unix Timestamp
    ThinEdgeIO.Transfer To Device    ${FILE}    /etc/tedge/operations/${OPERATION}.toml
    Should Have MQTT Messages
    ...    te/device/main///cmd/${OPERATION}
    ...    pattern="^{}$"
    ...    date_from=${timestamp}
    ...    timeout=60
