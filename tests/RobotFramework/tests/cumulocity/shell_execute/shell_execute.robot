*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting


*** Test Cases ***
Supports the c8y_Command operation out of the box
    Cumulocity.Should Contain Supported Operations    c8y_Command
    File Should Exist    /etc/tedge/operations/shell_execute.toml
    File Should Exist    /etc/tedge/operations/c8y/c8y_Command.template
    Symlink Should Exist    /etc/tedge/operations/c8y/c8y_Command

Executes a command and reports its output
    ${operation}=    Cumulocity.Create Operation
    ...    description=echo helloworld
    ...    fragments={"c8y_Command":{"text":"echo helloworld"}}
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    helloworld\n

Fails when the command returns a non-zero exit code
    ${operation}=    Cumulocity.Create Operation
    ...    description=failing command
    ...    fragments={"c8y_Command":{"text":"echo oops >&2; exit 1"}}
    Operation Should Be FAILED    ${operation}    failure_reason=.*Command returned exit code 1: oops.*

Truncates an output larger than the configured limit
    Execute Command    tedge config set shell.max_output_size 64
    ${operation}=    Cumulocity.Create Operation
    ...    description=print a large output
    ...    fragments={"c8y_Command":{"text":"yes hello | head -n 1000"}}
    Operation Should Be SUCCESSFUL    ${operation}
    Should Contain
    ...    ${operation.to_json()["c8y_Command"]["result"]}
    ...    <the output has been truncated after 64 bytes>

Reports a large non ASCII output without crashing the mapper
    [Documentation]    The result is trimmed by the mapper to fit the Cumulocity payload limit,
    ...    which must not cut a multi byte character in half
    Execute Command    tedge config set shell.max_output_size 65536
    ${operation}=    Cumulocity.Create Operation
    ...    description=print a large non ascii output
    ...    fragments={"c8y_Command":{"text":"yes 'éé' | head -n 20000"}}
    Operation Should Be SUCCESSFUL    ${operation}
    Service Health Status Should Be Up    tedge-mapper-c8y

Executes the command using the shell configured in tedge.toml
    Execute Command    cmd=printf '#!/bin/sh\\necho custom shell\\n' >/usr/bin/tedge-test-shell
    Execute Command    chmod a+x /usr/bin/tedge-test-shell
    Execute Command    tedge config set shell.path /usr/bin/tedge-test-shell

    ${operation}=    Cumulocity.Create Operation
    ...    description=echo helloworld
    ...    fragments={"c8y_Command":{"text":"echo helloworld"}}
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    custom shell\n

Supports the shell_execute command without any cloud
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/shell_execute/local-1234 '{"status":"init","command":"echo hello"}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/shell_execute/local-1234
    ...    message_contains="status":"successful"
    ...    minimum=1
    Should Contain    ${messages[0]}    hello
    [Teardown]    Execute Command    tedge mqtt pub --retain te/device/main///cmd/shell_execute/local-1234 ''

Supports disabling the Cumulocity c8y_Command operation
    Execute Command    tedge config set c8y.enable.shell_execute false
    Execute Command    rm -f /etc/tedge/operations/c8y/c8y_Command

    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y
    Should Not Contain Supported Operations    c8y_Command

    ${operation}=    Cumulocity.Create Operation
    ...    description=echo helloworld
    ...    fragments={"c8y_Command":{"text":"echo helloworld"}}
    Operation Should Be PENDING    ${operation}    timeout=30

    # Cleanup the pending operation, as it would otherwise pollute the test report output
    Execute Command    tedge mqtt pub c8y/s/us '505,${operation.to_json()["id"]},Cancelled operation'
    Operation Should Be FAILED    ${operation}

Restores the workflow definition when it has been removed
    Execute Command    rm -f /etc/tedge/operations/shell_execute.toml
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    File Should Exist    /etc/tedge/operations/shell_execute.toml

Supports disabling the shell_execute command on the device
    Execute Command    touch /etc/tedge/operations/shell_execute.toml.disabled
    Execute Command    rm -f /etc/tedge/operations/shell_execute.toml
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    File Should Not Exist    /etc/tedge/operations/shell_execute.toml


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Test Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
