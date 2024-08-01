*** Settings ***
Resource            ../../resources/common.resource
Library    ThinEdgeIO

Test Tags           theme:tedge_agent
Test Setup          Custom Setup
Test Teardown       Get Logs

*** Test Cases ***

Run agent with a custom topic prefix #3031
    # Only run tedge-agent for a few seconds
    ${output}=    Execute Command    timeout 5 tedge-agent --mqtt-topic-root "custom_root" --mqtt-device-topic-id "device/customname//" 2>&1    ignore_exit_code=${True}
    Should Not Contain    ${output}    te/
    Should Contain    ${output}    custom_root/device/customname//


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
     Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect
    Set Suite Variable    $DEVICE_SN
