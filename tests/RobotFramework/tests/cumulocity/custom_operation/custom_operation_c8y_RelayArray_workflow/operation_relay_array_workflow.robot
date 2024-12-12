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
Run c8y_RelayArray operation with workflow execution
    Symlink Should Exist    /etc/tedge/operations/c8y/c8y_RelayArray
    Cumulocity.Should Contain Supported Operations    c8y_RelayArray

    ${operation}=    Cumulocity.Create Operation
    ...    description=Set relays
    ...    fragments={"c8y_RelayArray":["OPEN", "CLOSED"]}

    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_pattern=^506,[0-9]+,OPEN,CLOSED
    ...    minimum=1
    ...    maximum=1
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}
    Cumulocity.Managed Object Should Have Fragment Values    c8y_RelayArray\=["OPEN", "CLOSED"]


*** Keywords ***
Transfer Configuration Files
    Transfer To Device    ${CURDIR}/c8y_RelayArray.template    /etc/tedge/operations/c8y/
    Transfer To Device    ${CURDIR}/set_relay.toml    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/set_relay.sh    /etc/tedge/operations/
    Execute Command    chmod a+x /etc/tedge/operations/set_relay.sh

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
