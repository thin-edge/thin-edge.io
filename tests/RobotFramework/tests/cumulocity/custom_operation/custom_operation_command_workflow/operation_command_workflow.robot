*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Variables ***
${PARENT_IP}    ${EMPTY}
${PARENT_SN}    ${EMPTY}


*** Test Cases ***
Run c8y_Command operation with workflow execution
    Symlink Should Exist    /etc/tedge/operations/c8y/c8y_Command
    Cumulocity.Should Contain Supported Operations    c8y_Command

    ${operation}=    Cumulocity.Create Operation
    ...    description=echo helloworld
    ...    fragments={"c8y_Command":{"text":"echo helloworld"}}

    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_pattern=^506,[0-9]+($|,\\"helloworld\n\\")
    ...    minimum=1
    ...    maximum=1
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Transfer Configuration Files
    Transfer To Device    ${CURDIR}/c8y_Command.template    /etc/tedge/operations/c8y/
    Transfer To Device    ${CURDIR}/shell_execute.toml    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/shell_execute.sh    /etc/tedge/operations/
    Execute Command    chmod a+x /etc/tedge/operations/shell_execute.sh

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=False
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}

    Set Device Context    ${PARENT_SN}
    Transfer Configuration Files
    Execute Command    tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    tedge config set mqtt.external.bind.port 1883
    Execute Command    tedge reconnect c8y

    Cumulocity.Device Should Exist    ${PARENT_SN}
