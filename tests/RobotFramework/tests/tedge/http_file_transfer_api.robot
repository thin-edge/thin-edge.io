*** Comments ***
# Command to execute:    robot -d \results --timestampoutputs --log http_file_transfer_api.html --report NONE -v BUILD:840 -v HOST:192.168.1.130 thin-edge.io/tests/RobotFramework/tedge/http_file_transfer_api.robot


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:cli    theme:configuration    theme:childdevices


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Parent device serial number
${DEVICE_IP}    ${EMPTY}    # Parent device host name which is reachable
${PORT}         8000


*** Test Cases ***
Get Put Delete legacy
    Setup    skip_bootstrap=True    # Setup child device

    ${put}=    Execute Command
    ...    curl -v -X PUT -d "test of put" http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a
    ...    stderr=True
    Should Contain    ${put}[1]    deprecation: true
    Should Contain    ${put}[1]    sunset: Thu, 31 Dec 2025 23:59:59 GMT
    Should Contain    ${put}[1]    link: </te/v1/files>; rel="deprecation"

    ${get}=    Execute Command
    ...    curl --silent -v http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a
    ...    stderr=True
    Should Contain    ${get}[0]    test of put
    Should Contain    ${get}[1]    deprecation: true
    Should Contain    ${get}[1]    sunset: Thu, 31 Dec 2025 23:59:59 GMT
    Should Contain    ${get}[1]    link: </te/v1/files>; rel="deprecation"

    ${delete}=    Execute Command
    ...    curl -v -X DELETE http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a
    ...    stderr=True
    Should Contain    ${delete}[1]    deprecation: true
    Should Contain    ${delete}[1]    sunset: Thu, 31 Dec 2025 23:59:59 GMT
    Should Contain    ${delete}[1]    link: </te/v1/files>; rel="deprecation"

Get Put Delete
    Setup    skip_bootstrap=True    # Setup child device

    ${put}=    Execute Command
    ...    curl -X PUT -d "test of put" http://${DEVICE_IP}:${PORT}/te/v1/files/file_a
    ...    stderr=True
    Should Not Contain    ${put}[1]    deprecation: true
    ${get}=    Execute Command    curl --silent http://${DEVICE_IP}:${PORT}/te/v1/files/file_a    stderr=True
    Should Be Equal    ${get}[0]    test of put
    Should Not Contain    ${get}[1]    deprecation: true
    ${delete}=    Execute Command    curl -X DELETE http://${DEVICE_IP}:${PORT}/te/v1/files/file_a    stderr=True
    Should Not Contain    ${delete}[1]    deprecation: true

File transfer using tedge cli
    Setup    skip_bootstrap=False

    Execute Command    tedge http put /te/v1/files/file_b "content to be transferred"
    ${content}=    Execute Command    tedge http get /te/v1/files/file_b
    Should Be Equal    ${content}    content to be transferred
    Execute Command    tedge http delete /te/v1/files/file_b
    Execute Command    tedge http get /te/v1/files/file_b    exp_exit_code=1


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
    Get Suite Logs
