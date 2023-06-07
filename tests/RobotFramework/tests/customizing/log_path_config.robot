#Command to execute:    robot -d \results --timestampoutputs --log log_path_config.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io/tests/RobotFramework/customizinglog_path_config.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:configuration
Suite Setup            Custom Setup
Suite Teardown         Custom Teardown

*** Test Cases ***
Validate updated data path used by tedge-agent
    Execute Command    sudo tedge config set logs.path /var/test
    Restart Service    tedge-agent
    Directory Should Exist    /var/test/tedge/agent

*** Keywords ***
Custom Setup
    Setup
    Execute Command    sudo mkdir /var/test
    Execute Command    sudo chown tedge:tedge /var/test

Custom Teardown
    Execute Command    sudo tedge config unset logs.path
    Restart Service    tedge-agent
    Execute Command    sudo rm -rf /var/test
    Get Logs
