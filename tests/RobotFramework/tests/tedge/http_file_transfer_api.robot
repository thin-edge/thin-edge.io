*** Comments ***
#Command to execute:    robot -d \results --timestampoutputs --log http_file_transfer_api.html --report NONE -v BUILD:840 -v HOST:192.168.1.130 thin-edge.io/tests/RobotFramework/tedge/http_file_transfer_api.robot


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Force Tags          theme:cli    theme:configuration    theme:childdevices


*** Variables ***
${DEVICE_SN}    # Parent device serial number
${DEVICE_IP}    # Parent device host name which is reachable
${PORT}=        8000


*** Test Cases ***
Get Put Delete
    Setup    skip_bootstrap=True    # Setup child device

    Execute Command    curl -X PUT -d "test of put" http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a
    ${get}=    Execute Command    curl --silent http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a
    Should Be Equal    ${get}    test of put
    Execute Command    curl -X DELETE http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=False
    Set Suite Variable    $DEVICE_SN    ${DEVICE_SN}

    ${DEVICE_IP}=    Get IP Address
    Set Suite Variable    ${DEVICE_IP}

    Execute Command    sudo tedge config set mqtt.external.bind.address ${DEVICE_IP}
    ${bind}=    Execute Command    tedge config get mqtt.external.bind.address    strip=True
    Should Be Equal    ${bind}    ${DEVICE_IP}
    Execute Command    sudo -u tedge mkdir -p /var/tedge
    Restart Service    tedge-agent

Custom Teardown
    Set Device Context    ${DEVICE_SN}
    Execute Command    sudo rm -rf /var/tedge/file-transfer
    Get Logs
