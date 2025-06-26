*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_agent


*** Test Cases ***
Run agent with a custom topic prefix #3031
    # Only run tedge-agent for a few seconds
    ${output}=    Execute Command
    ...    timeout 5 tedge-agent --mqtt-topic-root "custom_root" --mqtt-device-topic-id "device/customname//" 2>&1
    ...    ignore_exit_code=${True}
    Should Not Contain    ${output}    te/
    Should Contain    ${output}    custom_root/device/customname//

tedge-agent should not subscribe to legacy topics when running as a child device #3034
    # Only run tedge-agent for a few seconds
    ${output}=    Execute Command
    ...    timeout 5 tedge-agent --mqtt-device-topic-id "device/pump//" 2>&1
    ...    ignore_exit_code=${True}
    Should Contain    ${output}    Running as a child device, tedge_to_te_converter and File Transfer Service disabled
    Should Not Contain    ${output}    item=${SPACE}tedge/


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    $DEVICE_SN
