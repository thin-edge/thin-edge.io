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

Copy Scripts
    ThinEdgeIO.Transfer To Device    ${CURDIR}/echo-as-json.sh    /etc/tedge/operations/
