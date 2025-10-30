*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs
Test Timeout        5 minutes

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Test Cases ***
Successful shell command with output
    ${operation}=    Cumulocity.Execute Shell Command    echo helloworld
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    helloworld\n

Check Successful shell command with literal double quotes output
    ${operation}=    Cumulocity.Execute Shell Command    echo \\"helloworld\\"
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    "helloworld"\n

Execute multiline shell command
    ${operation}=    Cumulocity.Execute Shell Command    echo "hello"${\n}echo "world"
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    hello\nworld\n

Failed shell command
    ${operation}=    Cumulocity.Execute Shell Command    exit 1
    Operation Should Be FAILED    ${operation}

Shell command succeeds if output is too large
    [Documentation]    Output should be trimmed by c8y mapper.
    ${operation}=    Cumulocity.Execute Shell Command    yes 'hello"' | head -n 100000
    Operation Should Be SUCCESSFUL    ${operation}
    ${result}=    Set Variable    ${operation.to_json()["c8y_Command"]["result"]}
    Should End With    ${result}    ...<trimmed>


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/command_handler.*    /etc/tedge/operations/command
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y_Command*    /etc/tedge/operations/c8y/
    ThinEdgeIO.Restart Service    tedge-agent
    ThinEdgeIO.Disconnect Then Connect Mapper    c8y
