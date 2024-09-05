*** Comments ***
# Command to execute:    robot -d \results --timestampoutputs --log log_path_config.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io/tests/RobotFramework/customizinglog_path_config.robot


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:cli    theme:configuration


*** Test Cases ***
Validate updated log path used by tedge-agent
    Execute Command    sudo tedge config set logs.path /var/test
    Restart Service    tedge-agent
    Directory Should Exist    /var/test/agent


*** Keywords ***
Custom Setup
    Setup

Custom Teardown
    Execute Command    sudo tedge config unset logs.path
    Restart Service    tedge-agent
    Execute Command    sudo rm -rf /var/test
    Get Logs
