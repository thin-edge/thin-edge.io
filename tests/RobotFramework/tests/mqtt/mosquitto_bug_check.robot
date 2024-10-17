*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Suite Setup         Setup
Suite Teardown      Get Logs


*** Test Cases ***
Mosquitto bug check
    Execute Command    sudo systemctl stop tedge-mapper-c8y.service
    Execute Command    tedge mqtt pub -q 1 te/device/main///e/test "{\\"text\\":\\"hello $(date +%s)\\"}"
    Execute Command    sudo systemctl start tedge-mapper-c8y.service
    Device Should Have Event/s    expected_text=hello
